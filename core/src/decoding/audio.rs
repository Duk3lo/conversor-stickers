//! Cutting the audio of a video without re-encoding it: the reduced FFmpeg of Android has no audio
//! encoders, so the track is copied as it is into a container that can hold its codec.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

use super::{FfmpegDecoder, ffmpeg_seconds, read_text, run, video_input};
use crate::error::DecodingError;

/// Longest sound WhatsApp accepts for a sticker.
pub const MAX_SOUND_SECONDS: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundContainer {
    /// AAC in an MP4 file, saved as `.m4a`.
    Mp4,
    /// Opus or Vorbis in a WebM file.
    Webm,
}

impl SoundContainer {
    /// Picks the container that can hold `codec` (the name `ffprobe` gives it). The reduced FFmpeg
    /// has no muxer for MP3 nor Ogg, so those codecs are refused instead of re-encoded.
    pub fn for_codec(codec: &str) -> Result<SoundContainer, DecodingError> {
        match codec.to_ascii_lowercase().as_str() {
            "aac" => Ok(SoundContainer::Mp4),
            "opus" | "vorbis" => Ok(SoundContainer::Webm),
            other => Err(DecodingError::AudioNotSupported {
                codec: if other == "mp3" {
                    "MP3".to_string()
                } else {
                    other.to_ascii_uppercase()
                },
            }),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            SoundContainer::Mp4 => "m4a",
            SoundContainer::Webm => "webm",
        }
    }

    fn muxer(self) -> &'static str {
        match self {
            SoundContainer::Mp4 => "mp4",
            SoundContainer::Webm => "webm",
        }
    }
}

/// Arguments of the `ffmpeg` call that copies the audio of `[start_s, start_s + duration_s)`.
/// The duration is capped to [`MAX_SOUND_SECONDS`].
pub(super) fn cut_arguments(
    input: [OsString; 6],
    start_s: f64,
    duration_s: f64,
    container: SoundContainer,
    output: &Path,
) -> Vec<OsString> {
    let duration_s = duration_s.min(MAX_SOUND_SECONDS);
    let mut arguments: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]
        .into_iter()
        .map(OsString::from)
        .collect();
    arguments.extend(["-ss".into(), ffmpeg_seconds(start_s).into()]);
    arguments.extend(input);
    // Output-side `-ss 0` drops the packets before the requested start that input seeking keeps
    // (WebM/Opus starts at the previous keyframe), and output-side `-t` caps the real length.
    arguments.extend(["-ss".into(), "0".into()]);
    arguments.extend(["-t".into(), ffmpeg_seconds(duration_s).into()]);
    arguments
        .extend(["-vn", "-sn", "-dn", "-map", "0:a:0", "-c:a", "copy", "-f"].map(OsString::from));
    arguments.push(container.muxer().into());
    arguments.push(output.into());
    arguments
}

/// First line that `ffprobe` printed, which is the codec of the first audio track.
fn interpret_codec(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_ascii_lowercase)
}

impl FfmpegDecoder {
    /// Codec of the first audio track, or `None` if the video has no sound.
    pub fn audio_codec(
        &self,
        cancellation: Option<&AtomicBool>,
    ) -> Result<Option<String>, DecodingError> {
        let path = self.validate_file()?;
        let mut command = Command::new(&self.ffprobe_path);
        command
            .args(["-v", "error", "-select_streams", "a:0"])
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
        Ok(interpret_codec(&output))
    }

    /// Copies the audio of the clip into `{output_stem}.{m4a|webm}` and answers that path. It does
    /// not re-encode: a codec that has no container in the reduced FFmpeg is an error.
    pub fn cut_audio(
        &self,
        start_s: f64,
        duration_s: f64,
        output_stem: &Path,
        cancellation: &AtomicBool,
    ) -> Result<PathBuf, DecodingError> {
        let codec = self
            .audio_codec(Some(cancellation))?
            .ok_or(DecodingError::WithoutAudioTrack)?;
        let container = SoundContainer::for_codec(&codec)?;
        let path = self.validate_file()?;
        let mut output = output_stem.as_os_str().to_owned();
        output.push(".");
        output.push(container.extension());
        let output = PathBuf::from(output);
        let mut command = Command::new(&self.ffmpeg_path);
        command.args(cut_arguments(
            video_input(&path),
            start_s,
            duration_s,
            container,
            &output,
        ));
        run(
            command,
            Stdio::null(),
            &self.ffmpeg_path,
            self.time_limit,
            Some(cancellation),
            read_text,
        )?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aac_goes_to_m4a_and_opus_or_vorbis_to_webm() {
        assert_eq!(
            SoundContainer::for_codec("aac").unwrap(),
            SoundContainer::Mp4
        );
        assert_eq!(SoundContainer::Mp4.extension(), "m4a");
        for codec in ["opus", "vorbis", "Opus"] {
            assert_eq!(
                SoundContainer::for_codec(codec).unwrap(),
                SoundContainer::Webm
            );
        }
        assert_eq!(SoundContainer::Webm.extension(), "webm");
    }

    #[test]
    fn mp3_and_unknown_codecs_are_refused_in_spanish() {
        let mp3 = SoundContainer::for_codec("mp3").unwrap_err();
        assert_eq!(
            mp3.to_string(),
            "Este video tiene el audio en MP3 y aún no se puede guardar"
        );
        let flac = SoundContainer::for_codec("flac").unwrap_err();
        assert!(flac.to_string().contains("FLAC"));
    }

    #[test]
    fn arguments_copy_the_stream_and_cap_the_duration() {
        let input = video_input(Path::new("/tmp/clip.mp4"));
        let arguments = cut_arguments(
            input,
            2.5,
            30.0,
            SoundContainer::Mp4,
            Path::new("/tmp/sonido.m4a"),
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
        assert_eq!(after("-map"), "0:a:0");
        assert_eq!(after("-c:a"), "copy");
        let input_at = text.iter().position(|a| a == "-i").unwrap();
        assert!(text.iter().position(|a| a == "-t").unwrap() > input_at);
        assert_eq!(after("-f"), "mp4");
        assert!(text.contains(&"-vn".to_string()));
        assert_eq!(text.last().unwrap(), "/tmp/sonido.m4a");
        assert!(text.iter().position(|a| a == "-ss") < text.iter().position(|a| a == "-i"));
    }

    #[test]
    #[ignore = "needs ffmpeg in the PATH"]
    fn cutting_opus_in_webm_keeps_the_requested_duration() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("t.webm");
        let made = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc=d=30:r=25"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=d=30",
                "-c:v",
                "libvpx",
                "-g",
                "250",
            ])
            .args(["-c:a", "libopus"])
            .arg(&source)
            .status()
            .unwrap();
        assert!(made.success());
        let output = directory.path().join("o.webm");
        let arguments = cut_arguments(
            video_input(&source),
            12.3,
            5.0,
            SoundContainer::Webm,
            &output,
        );
        assert!(
            Command::new("ffmpeg")
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&output)
            .output()
            .unwrap();
        let seconds: f64 = String::from_utf8_lossy(&probe.stdout)
            .trim()
            .parse()
            .unwrap();
        assert!((4.5..5.5).contains(&seconds), "{seconds}");
    }

    #[test]
    fn webm_uses_the_webm_muxer() {
        let arguments = cut_arguments(
            video_input(Path::new("/tmp/clip.mkv")),
            0.0,
            4.0,
            SoundContainer::Webm,
            Path::new("/tmp/sonido.webm"),
        );
        let position = arguments.iter().position(|a| a == "-f").unwrap();
        assert_eq!(arguments[position + 1], "webm");
    }

    #[test]
    fn reads_the_codec_of_the_first_audio_track() {
        assert_eq!(interpret_codec("aac\n"), Some("aac".to_string()));
        assert_eq!(
            interpret_codec("\n  Opus \nmp3\n"),
            Some("opus".to_string())
        );
        assert_eq!(interpret_codec(""), None);
    }
}
