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

/// Longer side of the whole-frame still.
const STILL_SIDE_PX: u32 = 640;

/// Stills kept per entry: dragging the clip start asks for many frames, only the latest matter.
const STILLS_KEPT: usize = 8;

/// Last moment a still may be taken, so the request never lands past the final frame.
const STILL_MARGIN_S: f64 = 0.05;

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

/// Tenths of a second that name the still, and the second actually decoded: the request rounded to
/// 0.1 s and kept before the end of the video. `None` when it is outside `0..=duration_s`.
fn still_moment(at_s: f64, duration_s: f64) -> Option<(u64, f64)> {
    if !at_s.is_finite() || at_s < 0.0 || at_s > duration_s {
        return None;
    }
    // Finite and within the duration, so the rounding fits.
    let tenths = (at_s * 10.0).round() as u64;
    let latest_s = (duration_s - STILL_MARGIN_S).max(0.0);
    Some((tenths, (tenths as f64 / 10.0).min(latest_s)))
}

/// Deletes the oldest stills so that `STILLS_KEPT` remain, `current` among them.
fn trim_stills(directory: &Path, current: &Path) -> io::Result<()> {
    let mut others = Vec::new();
    for item in fs::read_dir(directory)? {
        let item = item?;
        let name = item.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("still-") && name.ends_with(".png")) || item.path() == current {
            continue;
        }
        others.push((item.metadata()?.modified()?, item.path()));
    }
    others.sort_by(|a, b| b.cmp(a));
    for (_, path) in others.into_iter().skip(STILLS_KEPT - 1) {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn thumbnail_error(error: impl std::fmt::Display) -> UiError {
    UiError::new(
        UiErrorKind::Other,
        format!("no se pudieron crear las miniaturas: {error}"),
    )
}

impl Session {
    /// Path of a PNG of the frame at `at_s`, with the video's own aspect ratio (no crop). Same
    /// folder and lifetime as the thumbnails; a second call for the same tenth of a second reuses it.
    pub(crate) fn video_still(&self, id: u32, at_s: f64) -> Result<PathBuf, UiError> {
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
        let Some((tenths, second_s)) = still_moment(at_s, duration_s) else {
            return Err(UiError::new(
                UiErrorKind::InvalidOptions,
                format!("el segundo {at_s} está fuera del video"),
            ));
        };
        let path = directory.join(format!("still-{tenths}.png"));
        if path.is_file() {
            return Ok(path);
        }
        fs::create_dir_all(&directory).map_err(thumbnail_error)?;
        let image = self
            .executables()
            .decoder(&video)
            .whole_frame_at(second_s, STILL_SIDE_PX, &self.inspection_cancellation)
            .map_err(|error| match error {
                DecodingError::Cancelled => {
                    UiError::from(conversor_core::ConversionError::Cancelled)
                }
                other => thumbnail_error(other),
            })?;
        let png = image.to_png().map_err(thumbnail_error)?;
        write_atomic(&path, &png).map_err(thumbnail_error)?;
        trim_stills(&directory, &path).map_err(thumbnail_error)?;
        Ok(path)
    }

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

    #[test]
    fn video_still_keeps_the_aspect_ratio_is_cached_and_validates_the_second() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 3.0);

        let path = session.video_still(id, 1.5).unwrap();

        let png = fs::read(&path).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        // IHDR holds the width and the height as big-endian u32 right after its tag.
        assert_eq!(png[16..20], 320u32.to_be_bytes());
        assert_eq!(png[20..24], 180u32.to_be_bytes());
        assert_eq!(session.video_still(id, 1.5).unwrap(), path);

        for bad in [f64::NAN, f64::INFINITY, -0.1, 3.5] {
            let error = session.video_still(id, bad).unwrap_err();
            assert_eq!(error.kind, UiErrorKind::InvalidOptions, "{bad}");
        }
        let error = session.video_still(id + 99, 1.0).unwrap_err();
        assert_eq!(error.kind, UiErrorKind::InvalidEntry);

        drop(session.remove_entry(id).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn still_moment_rounds_to_tenths_and_stays_before_the_end() {
        assert_eq!(still_moment(1.46, 3.0), Some((15, 1.5)));
        assert_eq!(still_moment(3.0, 3.0), Some((30, 2.95)));
        assert_eq!(still_moment(2.97, 3.0), Some((30, 2.95)));
        assert_eq!(still_moment(0.0, 0.02), Some((0, 0.0)));
        assert_eq!(still_moment(3.01, 3.0), None);
        assert_eq!(still_moment(-0.01, 3.0), None);
        assert_eq!(still_moment(f64::NAN, 3.0), None);
    }

    #[test]
    fn stills_at_the_same_tenth_share_a_file_and_the_end_of_the_video_works() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 3.0);
        let duration_s = session
            .state()
            .entry(id)
            .unwrap()
            .video
            .as_ref()
            .unwrap()
            .info
            .duration_s;

        let first = session.video_still(id, 1.02).unwrap();
        assert_eq!(session.video_still(id, 0.98).unwrap(), first);
        assert!(first.ends_with("still-10.png"));
        assert!(session.video_still(id, duration_s).unwrap().is_file());
    }

    #[test]
    fn only_the_latest_stills_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        let (session, id) = session_with_video(directory.path(), 3.0);

        let mut paths = Vec::new();
        for tenth in 0..12 {
            paths.push(session.video_still(id, f64::from(tenth) / 10.0).unwrap());
        }

        let folder = paths[0].parent().unwrap();
        let remaining = fs::read_dir(folder).unwrap().count();
        assert_eq!(remaining, STILLS_KEPT);
        assert!(paths.last().unwrap().is_file());
    }
}
