//! Video preview for the interface: WebKitGTK cannot play `<video>` through `asset:`, so `ffmpeg`
//! makes a small copy of a window of the video and the interface receives it over IPC and plays it
//! as a `blob:`. The original video never goes through IPC.
//!
//! The `ffmpeg` stderr is not read: it carries the path, which can contain the video title.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::{fs, io, thread};

use conversor_core::readable_size;
use tempfile::TempPath;
use tracing::warn;

use super::Session;
use crate::contract::{UiError, UiErrorKind};

const WINDOW_S: f64 = 120.0;

const SIDE_PX: u32 = 320;

const FPS: u32 = 15;

const MAX_SIZE_BYTES: u64 = 16_000_000;

const DEADLINE: Duration = Duration::from_secs(60);

const INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
enum PreviewError {
    #[error("se canceló la vista previa")]
    Cancelled,
    #[error("no se encontró `ffmpeg`; instálalo para ver la vista previa")]
    FfmpegNotFound,
    #[error("no se pudo lanzar ffmpeg para la vista previa: {0}")]
    ProcessFailed(#[source] io::Error),
    #[error("ffmpeg no pudo crear la vista previa ({process_end})")]
    FfmpegFailed { process_end: String },
    #[error("la vista previa tardó más de {} s", limit.as_secs())]
    TimedOut { limit: Duration },
    #[error("la vista previa pesa más de {}", readable_size(*max_bytes))]
    TooLarge { max_bytes: u64 },
    #[error("no se pudo usar el temporal de la vista previa: {0}")]
    Temp(#[source] io::Error),
}

impl From<PreviewError> for UiError {
    fn from(error: PreviewError) -> Self {
        let kind = match &error {
            PreviewError::Cancelled => UiErrorKind::Cancelled,
            PreviewError::FfmpegNotFound => UiErrorKind::FfmpegNotFound,
            PreviewError::TimedOut { .. } => UiErrorKind::TimedOut,
            PreviewError::FfmpegFailed { .. } | PreviewError::TooLarge { .. } => {
                UiErrorKind::InvalidEntry
            }
            PreviewError::ProcessFailed(_) | PreviewError::Temp(_) => UiErrorKind::Other,
        };
        UiError::new(kind, error.to_string())
    }
}

pub(super) struct Previewer {
    executable: PathBuf,
    deadline: Duration,
    max_size_bytes: u64,
}

impl Default for Previewer {
    fn default() -> Self {
        Previewer {
            executable: PathBuf::from("ffmpeg"),
            deadline: DEADLINE,
            max_size_bytes: MAX_SIZE_BYTES,
        }
    }
}

impl Previewer {
    fn generate(
        &self,
        video: &Path,
        from_s: f64,
        destination: TempPath,
        cancellation: &AtomicBool,
    ) -> Result<Vec<u8>, PreviewError> {
        let child =
            self.command(video, from_s, &destination)
                .spawn()
                .map_err(|error| match error.kind() {
                    io::ErrorKind::NotFound => PreviewError::FfmpegNotFound,
                    _ => PreviewError::ProcessFailed(error),
                })?;
        let mut process = Ffmpeg(child);
        let start = Instant::now();
        let state = loop {
            if cancellation.load(Ordering::Relaxed) {
                return Err(PreviewError::Cancelled);
            }
            if start.elapsed() >= self.deadline {
                return Err(PreviewError::TimedOut {
                    limit: self.deadline,
                });
            }
            match process.0.try_wait() {
                Ok(Some(state)) => break state,
                Ok(None) => thread::sleep(INTERVAL),
                Err(error) => return Err(PreviewError::ProcessFailed(error)),
            }
        };
        if !state.success() {
            return Err(PreviewError::FfmpegFailed {
                process_end: state.code().map_or_else(
                    || "terminado por una señal".to_string(),
                    |code| format!("código {code}"),
                ),
            });
        }
        let weighs = fs::metadata(&destination)
            .map_err(PreviewError::Temp)?
            .len();
        if weighs >= self.max_size_bytes {
            return Err(PreviewError::TooLarge {
                max_bytes: self.max_size_bytes,
            });
        }
        fs::read(&destination).map_err(PreviewError::Temp)
    }

    fn command(&self, video: &Path, from_s: f64, destination: &Path) -> Command {
        let filters = format!(
            "fps={FPS},scale=iw*sar:ih,setsar=1,crop=min(iw\\,ih):min(iw\\,ih),\
             scale={SIDE_PX}:{SIDE_PX}:flags=fast_bilinear,format=yuv420p"
        );
        let mut command = Command::new(&self.executable);
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-ss", &format!("{from_s:.3}")])
            .args(["-t", &format!("{WINDOW_S}")])
            .arg("-i")
            .arg(with_file_protocol(video))
            .args(["-an", "-sn", "-dn", "-map_metadata", "-1"])
            .args(["-vf", &filters])
            .args([
                "-c:v",
                "libvpx",
                "-deadline",
                "realtime",
                "-cpu-used",
                "-16",
            ])
            .args(["-b:v", "500k", "-maxrate", "600k", "-bufsize", "1200k"])
            .args(["-g", &FPS.to_string()])
            .args(["-threads", "1"])
            .args(["-fs", &self.max_size_bytes.to_string()])
            .args(["-f", "webm"])
            .arg(with_file_protocol(destination))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }
}

fn with_file_protocol(path: &Path) -> OsString {
    let mut argument = OsString::from("file:");
    argument.push(path);
    argument
}

struct Ffmpeg(Child);

impl Drop for Ffmpeg {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill() {
            warn!(%error, "no se pudo parar el ffmpeg de la vista previa");
        }
        if let Err(error) = self.0.wait() {
            warn!(%error, "no se pudo esperar al ffmpeg de la vista previa");
        }
    }
}

impl Session {
    pub(crate) fn preview(&self, id: u32, from_s: f64) -> Result<Vec<u8>, UiError> {
        let (video, destination, cancellation) = {
            let mut state = self.state();
            if state.closing {
                return Err(PreviewError::Cancelled.into());
            }
            let Some(video) = &state.entry(id)?.video else {
                return Err(UiError::new(
                    UiErrorKind::InvalidEntry,
                    format!("la entrada {id} no tiene un video listo"),
                ));
            };
            if !(from_s >= 0.0 && from_s < video.info.duration_s) {
                return Err(UiError::new(
                    UiErrorKind::InvalidOptions,
                    "la vista previa tiene que empezar dentro del video",
                ));
            }
            let path = video.path.clone();
            let temp = state.temp().map_err(PreviewError::Temp)?;
            let destination = tempfile::Builder::new()
                .prefix("preview-")
                .suffix(".webm")
                .tempfile_in(temp)
                .map_err(PreviewError::Temp)?
                .into_temp_path();
            state.cancel_preview();
            let cancellation = Arc::new(AtomicBool::new(false));
            state.preview = Some((id, Arc::clone(&cancellation)));
            state.previewing += 1;
            (path, destination, cancellation)
        };
        let _fin = PreviewEnd(self);
        let bytes = self
            .previewer
            .generate(&video, from_s, destination, &cancellation)?;
        Ok(bytes)
    }

    pub(crate) fn cancel_preview(&self) {
        self.state().cancel_preview();
    }
}

struct PreviewEnd<'a>(&'a Session);

impl Drop for PreviewEnd<'_> {
    fn drop(&mut self) {
        self.0.state().previewing -= 1;
        self.0.stopped_jobs.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    use tempfile::TempDir;

    use super::*;
    use crate::contract::UiErrorKind;
    use crate::session::Session;

    const TEST_DEADLINE: Duration = Duration::from_secs(5);

    fn ffmpeg(arguments: &[&str], output: &Path) {
        let done = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(arguments)
            .arg(output)
            .output()
            .expect("hace falta ffmpeg para estas pruebas");
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
    }

    fn video_with_audio(directory: &Path) -> PathBuf {
        let path = directory.join("titulo-secreto.mp4");
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=30:duration=3",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=3",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
                "-metadata",
                "title=titulo-secreto",
            ],
            &path,
        );
        path
    }

    fn long_video(directory: &Path) -> PathBuf {
        let path = directory.join("largo.mp4");
        let filter = "color=black:size=4x4:rate=2:duration=130,format=gray,\
                      geq=lum='min(255,2*T)',scale=64:64:flags=neighbor,format=yuv420p";
        ffmpeg(
            &["-f", "lavfi", "-i", filter, "-c:v", "mpeg4", "-q:v", "2"],
            &path,
        );
        path
    }

    fn session_with(previewer: Previewer, path: PathBuf) -> (Session, u32) {
        let session = Session {
            previewer,
            ..Session::default()
        };
        let id = session.add_files(vec![path])[0].id;
        (session, id)
    }

    struct Preview {
        _directory: TempDir,
        path: PathBuf,
    }

    impl Preview {
        fn new(bytes: &[u8]) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("previa.webm");
            fs::write(&path, bytes).unwrap();
            Preview {
                _directory: directory,
                path,
            }
        }

        fn ffprobe(&self, what: &str) -> String {
            let output = Command::new("ffprobe")
                .args(["-v", "error", "-of", "csv=p=0", "-show_entries", what])
                .arg(&self.path)
                .output()
                .unwrap();
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        }

        fn duration_s(&self) -> f64 {
            self.ffprobe("format=duration").parse().unwrap()
        }

        fn first_shine(&self) -> i32 {
            let output = Command::new("ffmpeg")
                .args(["-v", "error", "-nostdin", "-i"])
                .arg(&self.path)
                .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
                .output()
                .unwrap();
            i32::from(output.stdout[0])
        }
    }

    fn temp_files(session: &Session) -> usize {
        session
            .state()
            .temp
            .as_ref()
            .map_or(0, |temp| fs::read_dir(temp.path()).unwrap().count())
    }

    #[test]
    fn preview_is_small_square_webm_without_audio_or_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with(Previewer::default(), video_with_audio(directory.path()));

        let bytes = session.preview(id, 0.0).unwrap();

        assert_eq!(&bytes[..4], [0x1A, 0x45, 0xDF, 0xA3], "no es WebM");
        let preview = Preview::new(&bytes);
        assert_eq!(
            preview.ffprobe("stream=codec_type,codec_name,width,height"),
            format!("vp8,video,{SIDE_PX},{SIDE_PX}")
        );
        assert!((preview.duration_s() - 3.0).abs() < 0.2);
        let title = b"titulo-secreto";
        assert!(!bytes.windows(title.len()).any(|chunk| chunk == title));
        assert_eq!(temp_files(&session), 0, "el temporal se borra al leerlo");
    }

    #[test]
    fn window_starts_where_asked_and_reaches_end_of_video() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with(Previewer::default(), long_video(directory.path()));

        for (from_s, duration_s) in [(0.0, WINDOW_S), (60.0, 70.0), (120.0, 10.0)] {
            let preview = Preview::new(&session.preview(id, from_s).unwrap());

            assert!(
                (preview.duration_s() - duration_s).abs() < 1.0,
                "desde {from_s}: {}",
                preview.duration_s()
            );
            let expected = (from_s * 2.0) as i32;
            assert!(
                (preview.first_shine() - expected).abs() < 14,
                "desde {from_s}: {} en vez de {expected}",
                preview.first_shine()
            );
        }
    }

    #[test]
    fn window_is_the_one_the_interface_knows() {
        let examples: serde_json::Value =
            serde_json::from_str(include_str!("../../../src/contract.examples.json")).unwrap();
        assert_eq!(examples["preview_window_s"], WINDOW_S);
    }

    #[test]
    fn invalid_id_or_start_are_rejected_without_launching_ffmpeg() {
        let directory = tempfile::tempdir().unwrap();
        let previewer = Previewer {
            executable: directory.path().join("no-existe"),
            ..Previewer::default()
        };
        let (session, id) = session_with(previewer, video_with_audio(directory.path()));

        for from_s in [-0.1, f64::NAN, f64::INFINITY, 3.5] {
            let error = session.preview(id, from_s).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions, "{from_s}");
        }
        let error = session.preview(99, 0.0).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
        let pending = session.add_links("https://a.example/1").unwrap()[0].id;
        let error = session.preview(pending, 0.0).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
    }

    #[test]
    fn without_ffmpeg_or_on_failure_error_omits_video_path() {
        let directory = tempfile::tempdir().unwrap();
        let video = video_with_audio(directory.path());
        for (executable, kind) in [
            (
                directory.path().join("no-existe"),
                UiErrorKind::FfmpegNotFound,
            ),
            (PathBuf::from("false"), UiErrorKind::InvalidEntry),
        ] {
            let previewer = Previewer {
                executable,
                ..Previewer::default()
            };
            let (session, id) = session_with(previewer, video.clone());

            let error = session.preview(id, 0.0).unwrap_err();

            assert_eq!(error.kind, kind);
            assert!(
                !error.message.contains("titulo-secreto"),
                "{}",
                error.message
            );
            assert_eq!(temp_files(&session), 0);
        }
    }

    #[test]
    fn over_max_size_is_not_delivered() {
        let directory = tempfile::tempdir().unwrap();
        let previewer = Previewer {
            max_size_bytes: 10_000,
            ..Previewer::default()
        };
        let (session, id) = session_with(previewer, video_with_audio(directory.path()));

        let error = session.preview(id, 0.0).unwrap_err();

        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
        assert!(error.message.contains("10 KB"), "{}", error.message);
        assert_eq!(temp_files(&session), 0);
    }

    struct SlowFfmpeg {
        directory: TempDir,
    }

    impl SlowFfmpeg {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let dash = directory.path().join("lento.sh");
            fs::write(
                &dash,
                format!(
                    "#!/bin/sh\nfor destino; do :; done\necho parcial > \"$destino\"\n\
                     echo $$ >> '{}'\nexec sleep 30\n",
                    directory.path().join("pids").display()
                ),
            )
            .unwrap();
            let state = Command::new("install")
                .args(["-m", "755"])
                .arg(dash)
                .arg(directory.path().join("ffmpeg"))
                .status()
                .unwrap();
            assert!(state.success());
            SlowFfmpeg { directory }
        }

        fn session(&self, deadline: Duration) -> (Session, u32) {
            let previewer = Previewer {
                executable: self.directory.path().join("ffmpeg"),
                deadline,
                ..Previewer::default()
            };
            session_with(previewer, video_with_audio(self.directory.path()))
        }

        fn wait_for_pids(&self, how_many: usize) -> Vec<String> {
            let start = Instant::now();
            loop {
                let pids: Vec<String> = fs::read_to_string(self.directory.path().join("pids"))
                    .map(|text| text.lines().map(str::to_string).collect())
                    .unwrap_or_default();
                if pids.len() >= how_many {
                    return pids;
                }
                assert!(
                    start.elapsed() < TEST_DEADLINE,
                    "el ffmpeg falso no arrancó"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn died(pid: &str) -> bool {
        !Path::new("/proc").join(pid).exists()
    }

    #[test]
    fn cancel_kills_ffmpeg_and_deletes_partial_output() {
        let slow = SlowFfmpeg::new();
        let (session, id) = slow.session(DEADLINE);

        thread::scope(|threads| {
            let preview = threads.spawn(|| session.preview(id, 0.0));
            let pids = slow.wait_for_pids(1);
            session.cancel_preview();
            let error = preview.join().unwrap().unwrap_err();
            assert_eq!(error.kind, UiErrorKind::Cancelled);
            assert!(died(&pids[0]));
        });
        assert_eq!(temp_files(&session), 0);
    }

    #[test]
    fn requesting_another_cancels_the_previous() {
        let slow = SlowFfmpeg::new();
        let (session, id) = slow.session(DEADLINE);

        thread::scope(|threads| {
            let first = threads.spawn(|| session.preview(id, 0.0));
            slow.wait_for_pids(1);
            let second_one = threads.spawn(|| session.preview(id, 1.0));
            let error = first.join().unwrap().unwrap_err();
            assert_eq!(error.kind, UiErrorKind::Cancelled);
            let pids = slow.wait_for_pids(2);
            assert!(died(&pids[0]));
            assert!(!died(&pids[1]), "cancelar la primera paró la segunda");
            session.cancel_preview();
            assert!(second_one.join().unwrap().is_err());
        });
    }

    #[test]
    fn removing_video_cancels_its_preview() {
        let slow = SlowFfmpeg::new();
        let (session, id) = slow.session(DEADLINE);

        thread::scope(|threads| {
            let preview = threads.spawn(|| session.preview(id, 0.0));
            slow.wait_for_pids(1);
            session.remove_entry(id).unwrap();
            let error = preview.join().unwrap().unwrap_err();
            assert_eq!(error.kind, UiErrorKind::Cancelled);
        });
    }

    #[test]
    fn past_deadline_ffmpeg_is_killed() {
        let slow = SlowFfmpeg::new();
        let (session, id) = slow.session(Duration::from_secs(1));

        let error = session.preview(id, 0.0).unwrap_err();

        assert_eq!(error.kind, UiErrorKind::TimedOut);
        assert!(died(&slow.wait_for_pids(1)[0]));
        assert_eq!(temp_files(&session), 0);
    }

    #[test]
    fn close_waits_for_ffmpeg_to_die_and_deletes_session_temp() {
        let slow = SlowFfmpeg::new();
        let (session, id) = slow.session(DEADLINE);

        thread::scope(|threads| {
            let preview = threads.spawn(|| session.preview(id, 0.0));
            let pids = slow.wait_for_pids(1);
            let temp = session.state().temp.as_ref().unwrap().path().to_path_buf();
            session.close();
            assert!(died(&pids[0]), "al volver `cerrar`, ffmpeg ya no está");
            assert!(!temp.exists());
            assert!(preview.join().unwrap().is_err());
        });
        let error = session.preview(id, 0.0).unwrap_err();
        assert!(matches!(
            error.kind,
            UiErrorKind::Cancelled | UiErrorKind::InvalidEntry
        ));
        assert!(session.state().temp.is_none());
    }
}
