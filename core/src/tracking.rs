use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Decoding,
    Preview,
    FittingSize { attempt: u32, max: u32 },
    SearchingDuration { attempt: u32, max: u32 },
}

pub type PreviewReceiver<'a> = dyn Fn(&[u8]) -> io::Result<()> + Sync + 'a;

#[derive(Clone, Copy)]
pub struct Tracking<'a> {
    progress: &'a (dyn Fn(Progress) + Sync),
    cancellation: &'a AtomicBool,
    preview: Option<&'a PreviewReceiver<'a>>,
}

static CANCELLED_NEVER: AtomicBool = AtomicBool::new(false);

fn ignore(_: Progress) {}

impl<'a> Tracking<'a> {
    pub fn new(progress: &'a (dyn Fn(Progress) + Sync), cancellation: &'a AtomicBool) -> Self {
        Tracking {
            progress,
            cancellation,
            preview: None,
        }
    }

    pub fn with_preview(self, receiver: &'a PreviewReceiver<'a>) -> Self {
        Tracking {
            preview: Some(receiver),
            ..self
        }
    }

    pub fn none() -> Tracking<'static> {
        Tracking {
            progress: &ignore,
            cancellation: &CANCELLED_NEVER,
            preview: None,
        }
    }

    pub(crate) fn report(&self, stage: Progress) {
        (self.progress)(stage);
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancellation.load(Ordering::Relaxed)
    }

    pub(crate) fn cancellation(&self) -> &'a AtomicBool {
        self.cancellation
    }

    pub(crate) fn deliver_preview(&self, bytes: &[u8]) -> io::Result<()> {
        match self.preview {
            Some(receiver) => receiver(bytes),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn reports_each_stage_and_sees_cancellation() {
        let stages = Mutex::new(Vec::new());
        let progress = |stage| stages.lock().unwrap().push(stage);
        let cancellation = AtomicBool::new(false);
        let tracking = Tracking::new(&progress, &cancellation);

        tracking.report(Progress::Decoding);
        assert!(!tracking.cancelled());
        cancellation.store(true, Ordering::Relaxed);
        assert!(tracking.cancelled());
        assert_eq!(*stages.lock().unwrap(), [Progress::Decoding]);
    }

    #[test]
    fn preview_arrives_at_the_receiver_and_its_error_returns() {
        let received = Mutex::new(Vec::new());
        let save = |bytes: &[u8]| {
            received.lock().unwrap().push(bytes.to_vec());
            Ok(())
        };
        let cancellation = AtomicBool::new(false);
        let tracking = Tracking::new(&|_| {}, &cancellation).with_preview(&save);
        tracking.deliver_preview(b"webp").unwrap();
        assert_eq!(*received.lock().unwrap(), [b"webp".to_vec()]);

        let fail = |_: &[u8]| Err(io::Error::other("disco lleno"));
        let tracking = Tracking::new(&|_| {}, &cancellation).with_preview(&fail);
        assert!(tracking.deliver_preview(b"webp").is_err());
    }

    #[test]
    fn without_tracking_never_cancels() {
        let tracking = Tracking::none();
        tracking.report(Progress::Decoding);
        assert!(!tracking.cancelled());
        assert!(tracking.deliver_preview(b"webp").is_ok());
    }
}
