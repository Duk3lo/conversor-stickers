//! Converts a sticker downloaded from an external service (KLIPY) to the WhatsApp format with
//! the usual presets, without going through the session queue: the GIF if it is animated, the
//! PNG otherwise.

use std::sync::atomic::AtomicBool;

use conversor_core::{
    ConversionError, ConversionOptions, ConversionParams, Decoder as _, FramingSpec, Priority,
    Tracking, WHATSAPP_STATIC_STICKER, WHATSAPP_STICKER, convert, convert_image,
};

use crate::contract::UiError;
use crate::mobile::Executables;

pub(crate) fn to_whatsapp_sticker(
    path: &std::path::Path,
    animated: bool,
    executables: &Executables,
    cancellation: &AtomicBool,
) -> Result<Vec<u8>, UiError> {
    let no_progress = |_: conversor_core::Progress| {};
    let tracking = Tracking::new(&no_progress, cancellation);
    let decoder = executables.decoder(path);
    let to_ui = |error| UiError::from(ConversionError::from(error));
    if animated {
        let path = decoder.validate_file().map_err(to_ui)?;
        let info = executables
            .decoder(&path)
            .inspect_cancelable(cancellation)
            .map_err(to_ui)?;
        let priority = Priority::Balance;
        let params = ConversionParams {
            max_seconds: Some(info.duration_s.min(WHATSAPP_STICKER.max_duration_s)),
            fps: Some(priority.default_fps().value()),
            ..ConversionParams::for_preset(WHATSAPP_STICKER)
        };
        let options = ConversionOptions::new(&params)
            .map_err(|error| UiError::from(ConversionError::from(error)))?
            .with_priority(priority);
        let report =
            convert(&executables.decoder(&path), &options, tracking).map_err(UiError::from)?;
        Ok(report.bytes)
    } else {
        let image = decoder.decode_image(cancellation).map_err(to_ui)?;
        let report = convert_image(
            &image,
            &WHATSAPP_STATIC_STICKER,
            &FramingSpec::fit(),
            tracking,
        )
        .map_err(UiError::from)?;
        Ok(report.bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use conversor_core::{StickerKind, inspect_whatsapp_sticker};

    use super::*;

    fn with_ffmpeg(directory: &Path, name: &str, options: &[&str]) -> PathBuf {
        let path = directory.join(name);
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=size=200x200:rate=10"])
            .args(options)
            .arg(&path)
            .output()
            .expect("hace falta ffmpeg para estas pruebas");
        assert!(output.status.success(), "{output:?}");
        path
    }

    #[test]
    fn animated_gif_becomes_animated_sticker() {
        let directory = tempfile::tempdir().unwrap();
        let gif = with_ffmpeg(directory.path(), "a.gif", &["-t", "1"]);
        let bytes =
            to_whatsapp_sticker(&gif, true, &Executables::default(), &AtomicBool::new(false))
                .unwrap();
        let sticker = inspect_whatsapp_sticker(&bytes).unwrap();
        assert_eq!(sticker.kind, StickerKind::Animated);
    }

    #[test]
    fn png_becomes_static_sticker() {
        let directory = tempfile::tempdir().unwrap();
        let png = with_ffmpeg(directory.path(), "e.png", &["-frames:v", "1"]);
        let bytes = to_whatsapp_sticker(
            &png,
            false,
            &Executables::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let sticker = inspect_whatsapp_sticker(&bytes).unwrap();
        assert_eq!(sticker.kind, StickerKind::Static);
    }

    #[test]
    fn non_image_file_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("roto.gif");
        std::fs::write(&path, b"no es un gif").unwrap();
        assert!(
            to_whatsapp_sticker(
                &path,
                true,
                &Executables::default(),
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
}
