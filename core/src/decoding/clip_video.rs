//! Cutting a clip of a video, with its audio, without re-encoding it: the reduced FFmpeg of Android
//! has no video nor audio encoders, so the streams are copied into an MP4 file. The cut starts at
//! the keyframe before the requested start.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

use super::{FfmpegDecoder, MAX_SOUND_SECONDS, ffmpeg_seconds, read_text, run, video_input};
use crate::error::DecodingError;

/// Whether an MP4 file can hold the video codec (the name `ffprobe` gives it).
pub(super) fn check_video_codec(codec: &str) -> Result<(), DecodingError> {
    match codec.to_ascii_lowercase().as_str() {
        "h264" | "hevc" | "mpeg4" | "av1" => Ok(()),
        other => Err(DecodingError::VideoCodecNotSupported {
            codec: other.to_ascii_uppercase(),
        }),
    }
}

/// Arguments of the `ffmpeg` call that copies `[start_s, start_s + duration_s)` with the first
/// video track and, if there is one, the first audio track. The duration is capped to
/// [`MAX_SOUND_SECONDS`], the longest clip WhatsApp takes for a sticker.
pub(super) fn cut_video_arguments(
    input: [OsString; 6],
    start_s: f64,
    duration_s: f64,
    output: &Path,
) -> Vec<OsString> {
    let duration_s = duration_s.min(MAX_SOUND_SECONDS);
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]
        .into_iter()
        .map(OsString::from)
        .collect();
    arguments.extend(["-ss".into(), ffmpeg_seconds(start_s).into()]);
    arguments.extend(input);
    // Output-side, so the copied length is capped even though input seeking keeps the packets
    // from the keyframe before the start.
    arguments.extend(["-t".into(), ffmpeg_seconds(duration_s).into()]);
    arguments.extend(
        [
            "-c",
            "copy",
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
        ]
        .map(OsString::from),
    );
    arguments.push(output.into());
    arguments
}

impl FfmpegDecoder {
    /// Codec of the first video track.
    fn video_codec(&self, cancellation: Option<&AtomicBool>) -> Result<String, DecodingError> {
        let path = self.validate_file()?;
        let mut command = Command::new(&self.ffprobe_path);
        command
            .args(["-v", "error", "-select_streams", "v:0"])
            .args(["-show_entries", "stream=codec_name"])
            .args(["-of", "default=noprint_wrappers=1:nokey=1"])
            .args(video_input(&path));
        let output = run(
            command,
            Stdio::null(),
            &self.ffprobe_path,
            self.time_limit,
            cancellation,
            read_text,
        )?;
        output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_ascii_lowercase)
            .ok_or(DecodingError::WithoutVideoTrack)
    }

    /// Copies the clip, with its audio, into the MP4 file `output`. A video codec that MP4 does
    /// not take here is an error: there is no encoder to convert it.
    pub fn cut_video(
        &self,
        start_s: f64,
        duration_s: f64,
        output: &Path,
        cancellation: &AtomicBool,
    ) -> Result<(), DecodingError> {
        check_video_codec(&self.video_codec(Some(cancellation))?)?;
        let path = self.validate_file()?;
        let mut command = Command::new(&self.ffmpeg_path);
        command.args(cut_video_arguments(
            video_input(&path),
            start_s,
            duration_s,
            output,
        ));
        run(
            command,
            Stdio::null(),
            &self.ffmpeg_path,
            self.time_limit,
            Some(cancellation),
            read_text,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp4_codecs_are_accepted_and_the_rest_refused_in_spanish() {
        for codec in ["h264", "hevc", "mpeg4", "av1", "H264"] {
            assert!(check_video_codec(codec).is_ok(), "{codec}");
        }
        let error = check_video_codec("vp9").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Este video está en VP9 y no se puede guardar para un sticker con sonido"
        );
    }

    #[test]
    fn arguments_copy_both_streams_and_cap_the_duration() {
        let arguments = cut_video_arguments(
            video_input(Path::new("/tmp/clip.mp4")),
            2.5,
            30.0,
            Path::new("/tmp/clip-sticker.mp4"),
        );
        let text: Vec<String> = arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        let after = |flag: &str| {
            let position = text.iter().position(|argument| argument == flag).unwrap();
            text[position + 1].clone()
        };
        assert_eq!(after("-ss"), "2.500000");
        assert_eq!(after("-t"), "10.000000");
        assert_eq!(after("-c"), "copy");
        assert_eq!(after("-movflags"), "+faststart");
        assert_eq!(after("-f"), "mp4");
        assert!(text.contains(&"0:v:0".to_string()));
        assert!(text.contains(&"0:a:0?".to_string()));
        assert_eq!(text.last().unwrap(), "/tmp/clip-sticker.mp4");
        assert!(text.iter().position(|a| a == "-ss") < text.iter().position(|a| a == "-i"));
        assert!(text.iter().position(|a| a == "-t") > text.iter().position(|a| a == "-i"));
    }
}
