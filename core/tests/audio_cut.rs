#![allow(clippy::panic, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use conversor_core::{Decoder, DecodingError, FfmpegDecoder};
use tempfile::TempDir;

fn generate(path: &Path, audio: &[&str]) {
    let mut command = Command::new("ffmpeg");
    command
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=10:duration=4",
        ]);
    if audio.is_empty() {
        command.arg("-an");
    } else {
        command.args(["-f", "lavfi", "-i", "sine=frequency=440:duration=4"]);
        command.args(audio);
    }
    let output = command
        .args(["-c:v", "mpeg4", "-shortest"])
        .arg(path)
        .output()
        .expect("ffmpeg está instalado");
    assert!(
        output.status.success(),
        "ffmpeg no pudo generar {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn codec_and_duration(path: &Path) -> (String, f64) {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0"])
        .args(["-show_entries", "stream=codec_name:format=duration"])
        .args(["-of", "default=noprint_wrappers=1"])
        .arg(path)
        .output()
        .expect("ffprobe está instalado");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let value = |key: &str| {
        text.lines()
            .find_map(|line| {
                line.strip_prefix(key)
                    .and_then(|rest| rest.strip_prefix('='))
            })
            .unwrap_or_default()
            .to_string()
    };
    (
        value("codec_name"),
        value("duration").parse().expect("duración numérica"),
    )
}

#[test]
fn cuts_the_aac_of_a_video_without_reencoding_it() {
    let folder = TempDir::new().unwrap();
    let video = folder.path().join("con-audio.mp4");
    generate(&video, &["-c:a", "aac"]);
    let decoder = FfmpegDecoder::new(&video);

    assert!(decoder.inspect().unwrap().has_audio);
    let sound = decoder
        .cut_audio(
            1.0,
            2.0,
            &folder.path().join("sonido"),
            &AtomicBool::new(false),
        )
        .unwrap();

    assert_eq!(sound.extension().unwrap(), "m4a");
    let (codec, duration) = codec_and_duration(&sound);
    assert_eq!(codec, "aac");
    assert!((1.5..2.5).contains(&duration), "dura {duration} s");
}

#[test]
fn a_video_without_audio_says_so() {
    let folder = TempDir::new().unwrap();
    let video = folder.path().join("mudo.mp4");
    generate(&video, &[]);
    let decoder = FfmpegDecoder::new(&video);

    assert!(!decoder.inspect().unwrap().has_audio);
    let error = decoder
        .cut_audio(
            0.0,
            2.0,
            &folder.path().join("sonido"),
            &AtomicBool::new(false),
        )
        .unwrap_err();

    assert!(
        matches!(error, DecodingError::WithoutAudioTrack),
        "{error:?}"
    );
}

#[test]
fn mp3_audio_is_refused_with_a_clear_message() {
    let folder = TempDir::new().unwrap();
    let video = folder.path().join("con-mp3.mp4");
    generate(&video, &["-c:a", "libmp3lame"]);

    let error = FfmpegDecoder::new(&video)
        .cut_audio(
            0.0,
            2.0,
            &folder.path().join("sonido"),
            &AtomicBool::new(false),
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Este video tiene el audio en MP3 y aún no se puede guardar"
    );
}

#[test]
fn cuts_a_clip_with_video_and_audio_without_reencoding_it() {
    let folder = TempDir::new().unwrap();
    let video = folder.path().join("con-audio.mp4");
    generate(&video, &["-c:a", "aac"]);
    let output = folder.path().join("recorte.mp4");

    FfmpegDecoder::new(&video)
        .cut_video(0.0, 2.0, &output, &AtomicBool::new(false))
        .unwrap();

    let streams = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_name"])
        .args(["-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(&output)
        .output()
        .expect("ffprobe está instalado");
    let codecs: Vec<String> = String::from_utf8_lossy(&streams.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(codecs, ["mpeg4", "aac"]);
}

#[test]
fn a_clip_of_a_video_without_audio_keeps_only_the_video() {
    let folder = TempDir::new().unwrap();
    let video = folder.path().join("mudo.mp4");
    generate(&video, &[]);
    let output = folder.path().join("recorte.mp4");

    FfmpegDecoder::new(&video)
        .cut_video(0.0, 2.0, &output, &AtomicBool::new(false))
        .unwrap();

    assert!(output.metadata().unwrap().len() > 0);
}
