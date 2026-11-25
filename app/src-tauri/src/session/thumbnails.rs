//! Small frames evenly spaced over a video, for the clip bar. They are decoded as raw pixels by
//! `ffmpeg` and encoded to WebP here, because the `ffmpeg` bundled on Android has no image
//! encoders. They live in the session temp directory: one folder per entry, deleted with it.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::{fs, io};

use conversor_core::{DecodingError, encode_thumbnail_webp};
use tracing::warn;

use super::Session;
use crate::contract::{UiError, UiErrorKind};
use crate::storage::write_atomic;

const SIDE_PX: u32 = 160;

pub(crate) const MIN_COUNT: u32 = 4;

pub(crate) const MAX_COUNT: u32 = 16;

fn directory(temp: &Path, id: u32) -> PathBuf {
    temp.join(format!("thumbnails-{id}"))
}

fn file(directory: &Path, count: u32, index: u32) -> PathBuf {
    directory.join(format!("{count}-{index}.webp"))
}

/// Deletes the thumbnails of an entry that leaves the queue.
pub(super) fn forget(temp: &Path, id: u32) {
    let directory = directory(temp, id);
    match fs::remove_dir_all(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => warn!(%error, "no se pudieron borrar las miniaturas de un video quitado"),
    }
}

fn thumbnail_error(error: impl std::fmt::Display) -> UiError {
    UiError::new(
        UiErrorKind::Other,
        format!("no se pudieron crear las miniaturas: {error}"),
    )
}

impl Session {
    /// Paths of `count` thumbnails taken at the middle of `count` equal parts of the video, in
    /// order. A second call with the same `count` reuses the files.
    pub(crate) fn timeline_thumbnails(&self, id: u32, count: u32) -> Result<Vec<PathBuf>, UiError> {
        if !(MIN_COUNT..=MAX_COUNT).contains(&count) {
            return Err(UiError::new(
                UiErrorKind::InvalidOptions,
                format!("las miniaturas tienen que ser de {MIN_COUNT} a {MAX_COUNT}"),
            ));
        }
        let (video, duration_s, directory) = {
            let mut state = self.state();
            if state.closing {
                return Err(UiError::new(
                    UiErrorKind::Cancelled,
                    "se está cerrando la aplicación",
                ));
            }
            let Some(video) = &state.entry(id)?.video else {
                return Err(UiError::new(
                    UiErrorKind::InvalidEntry,
                    format!("la entrada {id} no tiene un video listo"),
                ));
            };
            let (path, duration_s) = (video.path.clone(), video.info.duration_s);
            let temp = state.temp().map_err(thumbnail_error)?;
            (path, duration_s, directory(temp, id))
        };
        let paths: Vec<PathBuf> = (0..count)
            .map(|index| file(&directory, count, index))
            .collect();
        if paths.iter().all(|path| path.is_file()) {
            return Ok(paths);
        }
        fs::create_dir_all(&directory).map_err(thumbnail_error)?;
        let decoder = self.executables().decoder(&video);
        for (index, path) in (0..count).zip(&paths) {
            let at_s = duration_s * (f64::from(index) + 0.5) / f64::from(count);
            let rgba = decoder
                .frame_at(at_s, SIDE_PX, &self.inspection_cancellation)
                .map_err(|error| match error {
                    DecodingError::Cancelled => {
                        UiError::from(conversor_core::ConversionError::Cancelled)
                    }
                    other => thumbnail_error(other),
                })?;
            let webp = encode_thumbnail_webp(&rgba, SIDE_PX).map_err(thumbnail_error)?;
            write_atomic(path, &webp).map_err(thumbnail_error)?;
            if self.inspection_cancellation.load(Ordering::Relaxed) {
                return Err(UiError::from(conversor_core::ConversionError::Cancelled));
            }
        }
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::contract::EntryState;

    fn session_with_video(directory: &Path, seconds: f64) -> (Session, u32) {
        let path = directory.join("video.mp4");
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30"])
            .args(["-t", &seconds.to_string()])
            .args([
                "-c:v", "mpeg4", "-q:v", "3", "-pix_fmt", "yuv420p", "-f", "mp4",
            ])
            .arg(&path)
            .output()
            .expect("hace falta ffmpeg para estas pruebas");
        assert!(output.status.success());
        let session = Session::default();
        let entries = session.add_files(vec![path]);
        assert_eq!(entries[0].state, EntryState::Ready);
        (session, entries[0].id)
    }

    fn side_of_webp(bytes: &[u8]) -> (u32, u32) {
        let decoder = webp_animation::Decoder::new(bytes).unwrap();
        decoder.dimensions()
    }

    #[test]
    fn makes_small_ordered_cached_thumbnails_and_removing_the_entry_deletes_them() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 3.0);

        let paths = session.timeline_thumbnails(id, 6).unwrap();

        assert_eq!(paths.len(), 6);
        for path in &paths {
            assert_eq!(side_of_webp(&fs::read(path).unwrap()), (SIDE_PX, SIDE_PX));
        }
        let modified = fs::metadata(&paths[0]).unwrap().modified().unwrap();
        assert_eq!(session.timeline_thumbnails(id, 6).unwrap(), paths);
        assert_eq!(
            fs::metadata(&paths[0]).unwrap().modified().unwrap(),
            modified
        );

        drop(session.remove_entry(id).unwrap());
        assert!(paths.iter().all(|path| !path.exists()));
    }

    #[test]
    fn closing_the_session_leaves_no_thumbnails() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 2.0);
        let paths = session.timeline_thumbnails(id, 4).unwrap();
        session.close();
        assert!(paths.iter().all(|path| !path.exists()));
    }

    #[test]
    fn count_outside_4_to_16_and_unknown_entries_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 1.0);
        for count in [0, 3, 17, 100] {
            let error = session.timeline_thumbnails(id, count).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions, "{count}");
            assert_eq!(error.message, "las miniaturas tienen que ser de 4 a 16");
        }
        let error = session.timeline_thumbnails(id + 99, 4).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);
    }
}
