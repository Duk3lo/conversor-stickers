#![allow(clippy::panic, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use conversor_core::{
    ConversionOptions, ConversionParams, Decoder, DecodingError, FfmpegDecoder, Fps, RgbaImage,
    WHATSAPP_STICKER, plan_time,
};

const MAX_MEAN_DIFFERENCE: f64 = 1.0;

fn reduced_ffmpeg() -> Option<PathBuf> {
    match std::env::var_os("CONVERSOR_FFMPEG_DIR") {
        Some(directory) => Some(PathBuf::from(directory)),
        None => {
            eprintln!("sin CONVERSOR_FFMPEG_DIR: se salta la paridad del FFmpeg reducido");
            None
        }
    }
}

fn reduced(directory: &Path, path: &Path) -> FfmpegDecoder {
    FfmpegDecoder::new(path).with_executables(directory.join("ffmpeg"), directory.join("ffprobe"))
}

fn generate(path: &Path, input: &str, arguments: &[&str]) {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-f", "lavfi", "-i", input])
        .args(arguments)
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

fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    sum as f64 / a.len() as f64
}

fn with_orientation_6(jpeg: &[u8]) -> Vec<u8> {
    assert_eq!(&jpeg[..2], [0xFF, 0xD8], "no es un JPEG");
    let mut app1 = vec![0xFF, 0xE1, 0x00, 0x22];
    app1.extend(b"Exif\0\0");
    app1.extend([0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]);
    app1.extend([0x01, 0x00]);
    app1.extend([
        0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00,
    ]);
    app1.extend([0x00, 0x00, 0x00, 0x00]);
    let mut result = jpeg[..2].to_vec();
    result.extend(app1);
    result.extend(&jpeg[2..]);
    result
}

#[test]
fn ffmpeg_reduced_decodes_videos_equal_as_system() {
    let Some(ffmpeg) = reduced_ffmpeg() else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let in_directory = |name: &str| directory.path().join(name);
    let pattern = "testsrc2=size=320x180:rate=30:duration=3";
    let cases: [(&str, &[&str]); 12] = [
        ("h264.mp4", &["-c:v", "libx264", "-pix_fmt", "yuv420p"]),
        (
            "hevc.mp4",
            &[
                "-c:v",
                "libx265",
                "-tag:v",
                "hvc1",
                "-x265-params",
                "log-level=error",
            ],
        ),
        ("v.mov", &["-c:v", "libx264", "-pix_fmt", "yuv420p"]),
        ("vp8.webm", &["-c:v", "libvpx", "-deadline", "realtime"]),
        ("vp9.webm", &["-c:v", "libvpx-vp9", "-deadline", "realtime"]),
        ("v.mkv", &["-c:v", "libx264", "-pix_fmt", "yuv420p"]),
        ("v.avi", &["-c:v", "mpeg4"]),
        ("v.gif", &[]),
        ("v.mpg", &["-c:v", "mpeg2video"]),
        ("v.ogv", &["-c:v", "libtheora"]),
        (
            "h264_10bits.mp4",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p10le"],
        ),
        (
            "hevc_10bits.mkv",
            &[
                "-c:v",
                "libx265",
                "-pix_fmt",
                "yuv420p10le",
                "-x265-params",
                "log-level=error",
            ],
        ),
    ];
    let mut videos = Vec::new();
    for (name, arguments) in cases {
        generate(&in_directory(name), pattern, arguments);
        videos.push(in_directory(name));
    }
    generate(
        &in_directory("con_audio.mp4"),
        &format!("{pattern}[out0];sine=duration=3[out1]"),
        &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"],
    );
    videos.push(in_directory("con_audio.mp4"));
    let state = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-display_rotation", "90", "-i"])
        .arg(in_directory("h264.mp4"))
        .args(["-c", "copy"])
        .arg(in_directory("girado.mp4"))
        .status()
        .expect("ffmpeg está instalado");
    assert!(state.success());
    videos.push(in_directory("girado.mp4"));

    let options = ConversionOptions::new(&ConversionParams::for_preset(WHATSAPP_STICKER)).unwrap();
    for video in &videos {
        let name = video.file_name().unwrap().to_string_lossy();
        let system = FfmpegDecoder::new(video);
        let reduced = reduced(&ffmpeg, video);

        let expected = system.inspect().unwrap();
        let obtained = reduced
            .inspect()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            (obtained.width, obtained.height),
            (expected.width, expected.height),
            "{name}"
        );
        assert!(
            (obtained.duration_s - expected.duration_s).abs() < 1e-3,
            "{name}"
        );

        let plan = plan_time(expected.duration_s, &options).unwrap();
        let decode = |decoder: &FfmpegDecoder| {
            decoder
                .decode(&plan, Fps::DEFAULT, 128, &AtomicBool::new(false))
                .unwrap_or_else(|error| panic!("{name}: {error}"))
        };
        let expected = decode(&system);
        let obtained = decode(&reduced);
        assert_eq!(obtained.frames().len(), expected.frames().len(), "{name}");
        for (index, (a, b)) in obtained.frames().iter().zip(expected.frames()).enumerate() {
            assert_eq!(a.duration_ms, b.duration_ms, "{name}, frame {index}");
            let difference = mean_difference(&a.rgba, &b.rgba);
            assert!(
                difference <= MAX_MEAN_DIFFERENCE,
                "{name}, frame {index}: diferencia media {difference}"
            );
        }
    }
}

#[test]
fn ffmpeg_reduced_decodes_images_equal_as_system() {
    let Some(ffmpeg) = reduced_ffmpeg() else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let in_directory = |name: &str| directory.path().join(name);
    let pattern = "testsrc2=size=320x180,format=rgba";
    let with_alpha =
        "testsrc2=size=320x180,format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='X*255/W'";
    generate(&in_directory("alfa.png"), with_alpha, &["-frames:v", "1"]);
    generate(&in_directory("foto.jpg"), pattern, &["-frames:v", "1"]);
    generate(
        &in_directory("foto.webp"),
        pattern,
        &["-frames:v", "1", "-c:v", "libwebp"],
    );
    generate(
        &in_directory("animado.webp"),
        "testsrc2=size=320x180:rate=10:duration=1",
        &["-c:v", "libwebp_anim", "-loop", "0"],
    );
    generate(&in_directory("foto.bmp"), pattern, &["-frames:v", "1"]);
    let jpeg = fs::read(in_directory("foto.jpg")).unwrap();
    fs::write(in_directory("girada.jpg"), with_orientation_6(&jpeg)).unwrap();

    let decode = |decoder: FfmpegDecoder, name: &str| -> RgbaImage {
        decoder
            .decode_image(&AtomicBool::new(false))
            .unwrap_or_else(|error| panic!("{name}: {error}"))
    };
    for name in [
        "alfa.png",
        "foto.jpg",
        "girada.jpg",
        "foto.webp",
        "animado.webp",
        "foto.bmp",
    ] {
        let path = in_directory(name);
        let expected_info = FfmpegDecoder::new(&path).inspect_image().unwrap();
        let info = reduced(&ffmpeg, &path)
            .inspect_image()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(info, expected_info, "{name}");

        let expected = decode(FfmpegDecoder::new(&path), name);
        let obtained = decode(reduced(&ffmpeg, &path), name);
        assert_eq!(
            (obtained.width(), obtained.height()),
            (expected.width(), expected.height()),
            "{name}"
        );
        let difference = mean_difference(obtained.rgba(), expected.rgba());
        assert!(
            difference <= MAX_MEAN_DIFFERENCE,
            "{name}: diferencia media {difference}"
        );
        if name == "girada.jpg" {
            assert_eq!(
                (obtained.width(), obtained.height()),
                (180, 320),
                "EXIF sin aplicar"
            );
        }
        if name == "alfa.png" {
            let min_alpha = obtained.rgba().chunks(4).map(|pixel| pixel[3]).min();
            assert_eq!(min_alpha, Some(0), "se perdió el alfa");
        }
    }
}

#[test]
fn ffmpeg_reduced_rejects_video_trap() {
    let Some(ffmpeg) = reduced_ffmpeg() else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let secret = directory.path().join("secreto.mp4");
    generate(
        &secret,
        "testsrc2=size=160x120:rate=25:duration=2",
        &["-c:v", "mpeg4"],
    );
    let trap = directory.path().join("inocente.mp4");
    fs::write(
        &trap,
        "ffconcat version 1.0\nfile secreto.mp4\nduration 2\n",
    )
    .unwrap();

    let result = reduced(&ffmpeg, &trap).inspect();
    assert!(
        matches!(result, Err(DecodingError::FfmpegFailed { .. })),
        "{result:?}"
    );
}
