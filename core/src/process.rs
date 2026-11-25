use std::io::{self, ErrorKind, Read};
use std::process::{Child, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use tracing::warn;

const SUMMARIZED_STDERR_LINES: usize = 5;

pub(crate) const STDERR_MAX_BYTES: usize = 64 * 1024;

pub(crate) struct WatchedProcess {
    pub(crate) child: Child,
    pub(crate) program: String,
    finished: bool,
    own_group: bool,
}

impl WatchedProcess {
    pub(crate) fn new(child: Child, program: String) -> Self {
        WatchedProcess {
            child,
            program,
            finished: false,
            own_group: false,
        }
    }

    pub(crate) fn with_own_group(child: Child, program: String) -> Self {
        WatchedProcess {
            child,
            program,
            finished: false,
            own_group: true,
        }
    }

    pub(crate) fn wait_until(
        &mut self,
        start: Instant,
        limit: Duration,
    ) -> io::Result<Option<ExitStatus>> {
        loop {
            if let Some(state) = self.child.try_wait()? {
                self.finished = true;
                return Ok(Some(state));
            }
            if start.elapsed() >= limit {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for WatchedProcess {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if self.own_group {
            self.kill_group();
        }
        if let Err(error) = self.child.kill() {
            warn!(program = %self.program, %error, "no se pudo matar el proceso");
        }
        if let Err(error) = self.child.wait() {
            warn!(program = %self.program, %error, "no se pudo esperar al proceso");
        }
    }
}

impl WatchedProcess {
    #[cfg(unix)]
    pub(crate) fn leader_finished(&mut self) -> io::Result<bool> {
        use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};

        let final_of_leader = waitid(
            WaitId::Pid(Pid::from_child(&self.child)),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )?;
        Ok(final_of_leader.is_some())
    }

    #[cfg(not(unix))]
    pub(crate) fn leader_finished(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }

    pub(crate) fn kill_group_and_wait(&mut self) -> io::Result<ExitStatus> {
        self.kill_group();
        let state = self.child.wait()?;
        self.finished = true;
        Ok(state)
    }

    #[cfg(unix)]
    fn kill_group(&self) {
        use rustix::io::Errno;
        use rustix::process::{Pid, Signal, kill_process_group};

        match kill_process_group(Pid::from_child(&self.child), Signal::KILL) {
            Ok(()) | Err(Errno::SRCH) => {}
            Err(error) => {
                warn!(program = %self.program, %error, "no se pudo matar el grupo de procesos");
            }
        }
    }

    #[cfg(not(unix))]
    fn kill_group(&self) {}
}

pub(crate) fn launch_thread<T: Send + 'static>(
    name: String,
    task: impl FnOnce() -> T + Send + 'static,
) -> io::Result<thread::JoinHandle<T>> {
    thread::Builder::new().name(name).spawn(task)
}

pub(crate) fn read_final_bounded(reader: &mut impl Read, max: usize) -> io::Result<Vec<u8>> {
    let mut last = Vec::new();
    let mut block = [0u8; 8192];
    loop {
        let read = match reader.read(&mut block) {
            Ok(0) => return Ok(last),
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        last.extend_from_slice(&block[..read]);
        if last.len() > max {
            let surplus = last.len() - max;
            last.drain(..surplus);
        }
    }
}

pub(crate) fn summarize_stderr(stderr: &str) -> String {
    let lines: Vec<String> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.chars()
                .map(|char| if char.is_control() { '?' } else { char })
                .collect()
        })
        .collect();
    let from = lines.len().saturating_sub(SUMMARIZED_STDERR_LINES);
    let summary = lines[from..].join(" | ");
    if summary.is_empty() {
        "sin mensaje de error".to_string()
    } else {
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_stderr_save_only_last_bytes() {
        let stderr: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        let last = read_final_bounded(&mut stderr.as_slice(), 1000).unwrap();
        assert_eq!(last, stderr[stderr.len() - 1000..]);
        let short = read_final_bounded(&mut b"poco".as_slice(), 1000).unwrap();
        assert_eq!(short, b"poco");
    }

    #[test]
    fn summary_of_stderr_changes_chars_of_control_by_question_marks() {
        let stderr = "ERROR: \u{1b}]52;c;bWFsbw==\u{7}fin\r\notra\u{0}\u{9b}31m";
        assert_eq!(
            summarize_stderr(stderr),
            "ERROR: ?]52;c;bWFsbw==?fin | otra??31m"
        );
    }

    #[test]
    fn summary_of_stderr_are_last_lines() {
        let stderr = "a\n\nb\nc\nd\ne\nf\n";
        assert_eq!(summarize_stderr(stderr), "b | c | d | e | f");
        assert_eq!(summarize_stderr("  \n"), "sin mensaje de error");
    }
}
