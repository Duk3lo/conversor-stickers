#![allow(clippy::panic, clippy::expect_used)]

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use conversor_core::{
    Clip, ConversionOptions, ConversionParams, Decoder, DecodingError, FfmpegDecoder, Fps,
    FramingMode, FramingSpec, ImageInfo, MAX_IMAGE_SIZE_BYTES, RgbaImage, TimePlan,
    WHATSAPP_STICKER, plan_time,
};
use tempfile::TempDir;

use common::{
    Content, ImageContent, ImageFormat, TestImage, TestVideo, ffprobe_duration, read_apng,
    read_gif, read_webp,
};

#[test]
fn video_generated_reuses_of_cache_without_regenerate_it() {
    let video = TestVideo::horizontal(3.0);
    let path = video.path();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();

    assert_eq!(video.path(), path);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert!(path.starts_with(common::directory_cache()));
}

#[test]
fn videos_generated_have_duration_requested() {
    for video in [
        TestVideo::vertical(3.0),
        TestVideo::new(Content::Static, 640, 360, 3.0),
        TestVideo::new(Content::Noise, 640, 360, 3.0),
        TestVideo::new(Content::CenterSquare { side_px: 120 }, 320, 360, 3.0).with_sar(2, 1),
    ] {
        let duration = ffprobe_duration(&video.path());
        assert!(
            (duration - video.duration_s).abs() < 0.1,
            "{}: {duration} s",
            video.file_name()
        );
    }
}

#[test]
fn name_in_cache_distinguishes_all_params() {
    let base = TestVideo::horizontal(19.5);
    assert_eq!(base.file_name(), "patron-640x360-sar1_1-19.5s-30fps.mp4");
    assert_ne!(base.file_name(), base.with_sar(2, 1).file_name());
    assert_ne!(base.file_name(), TestVideo::horizontal(20.0).file_name());
}

fn ffmpeg_animation(extension: &str, encoder: &[&str]) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("se puede crear un directorio temporal");
    let path = directory.path().join(format!("prueba.{extension}"));
    let state = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=10:duration=1",
        ])
        .args(encoder)
        .arg(&path)
        .status()
        .expect("ffmpeg está instalado");
    assert!(state.success(), "ffmpeg no pudo generar {}", path.display());
    (directory, path)
}

#[test]
fn reader_webp_returns_dimensions_frames_durations_and_size() {
    let (_directory, path) = ffmpeg_animation("webp", &["-c:v", "libwebp_anim", "-loop", "0"]);
    let animation = read_webp(&path);
    assert_eq!((animation.width, animation.height), (64, 48));
    assert_eq!(animation.frame_count(), 10);
    assert_eq!(animation.durations_ms, vec![100; 10]);
    assert_eq!(animation.total_duration_ms(), 1000);
    assert_eq!(animation.min_duration_ms(), Some(100));
    assert_eq!(animation.size_bytes, fs::metadata(&path).unwrap().len());
}

#[test]
fn reader_gif_returns_dimensions_frames_durations_and_size() {
    let (_directory, path) = ffmpeg_animation("gif", &["-c:v", "gif"]);
    let animation = read_gif(&path);
    assert_eq!((animation.width, animation.height), (64, 48));
    assert_eq!(animation.frame_count(), 10);
    assert_eq!(animation.durations_ms, vec![100; 10]);
    assert_eq!(animation.size_bytes, fs::metadata(&path).unwrap().len());
}

#[test]
fn reader_apng_returns_dimensions_frames_durations_and_size() {
    let (_directory, path) =
        ffmpeg_animation("apng", &["-c:v", "apng", "-plays", "0", "-f", "apng"]);
    let animation = read_apng(&path);
    assert_eq!((animation.width, animation.height), (64, 48));
    assert_eq!(animation.frame_count(), 10);
    assert_eq!(animation.durations_ms, vec![100; 10]);
    assert_eq!(animation.size_bytes, fs::metadata(&path).unwrap().len());
}

#[test]
#[should_panic(expected = "no es un APNG")]
fn reader_apng_rejects_png_fixed() {
    let (_directory, path) = ffmpeg_animation("png", &["-frames:v", "1"]);
    read_apng(&path);
}

const SIDE: u32 = 512;

fn plan_for(decoder: &FfmpegDecoder, params: ConversionParams) -> (TimePlan, Fps) {
    let info = decoder.inspect().expect("el video de prueba es válido");
    let options = ConversionOptions::new(&params).expect("parámetros válidos");
    (
        plan_time(info.duration_s, &options).expect("el tramo entra"),
        options.fps(),
    )
}

fn decode(video: &TestVideo, params: ConversionParams) -> (TimePlan, Fps, Clip) {
    let decoder = FfmpegDecoder::new(video.path());
    let (plan, fps) = plan_for(&decoder, params);
    let clip = decoder
        .decode(&plan, fps, SIDE, &AtomicBool::new(false))
        .expect("el video se decodifica");
    (plan, fps, clip)
}

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIDE + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn is_red([r, g, b, _]: [u8; 4]) -> bool {
    r > 180 && g < 80 && b < 80
}

fn is_blue([r, g, b, _]: [u8; 4]) -> bool {
    b > 180 && r < 80 && g < 80
}

fn red_range(pixels: impl Iterator<Item = [u8; 4]>) -> (usize, usize) {
    let red: Vec<usize> = pixels
        .enumerate()
        .filter(|&(_, p)| is_red(p))
        .map(|(i, _)| i)
        .collect();
    let first = *red.first().expect("hay píxeles rojos");
    let last = *red.last().expect("hay píxeles rojos");
    assert_eq!(red.len(), last - first + 1, "el tramo rojo es continuo");
    (first, red.len())
}

#[test]
fn horizontal_vertical_and_anamorphic_give_frames_squares_without_deform() {
    let square_side = 120;
    let content = Content::CenterSquare {
        side_px: square_side,
    };
    for video in [
        TestVideo::new(content, 640, 360, 3.0),
        TestVideo::new(content, 360, 640, 3.0),
        TestVideo::new(content, 320, 360, 3.0).with_sar(2, 1),
    ] {
        let (_, _, clip) = decode(&video, ConversionParams::for_preset(WHATSAPP_STICKER));
        assert_eq!(clip.side(), SIDE);
        let rgba = &clip.frames()[0].rgba;
        assert_eq!(rgba.len(), (SIDE * SIDE * 4) as usize);

        let expected = f64::from(square_side) * f64::from(SIDE) / 360.0;
        let center = SIDE / 2;
        let (x0, width) = red_range((0..SIDE).map(|x| pixel(rgba, x, center)));
        let (y0, height) = red_range((0..SIDE).map(|y| pixel(rgba, center, y)));
        let name = video.file_name();
        for (measure, origin) in [(width, x0), (height, y0)] {
            assert!(
                (measure as f64 - expected).abs() <= 4.0,
                "{name}: el cuadrado mide {width}x{height}, se esperaba {expected:.1}"
            );
            let expected_origin = (f64::from(SIDE) - expected) / 2.0;
            assert!(
                (origin as f64 - expected_origin).abs() <= 4.0,
                "{name}: el cuadrado empieza en ({x0}, {y0}), no está centrado"
            );
        }
        assert!(is_blue(pixel(rgba, 5, 5)), "{name}: el fondo es azul");
        assert_eq!(pixel(rgba, 5, 5)[3], 255, "{name}: el frame es opaco");
    }
}

#[test]
fn number_of_frames_is_floor_of_duration_by_fps() {
    let video = TestVideo::horizontal(3.0);
    for (start_s, fps, expected) in [(0.0, 15, 45), (1.0, 15, 30), (0.0, 10, 30), (0.5, 12, 30)] {
        let (plan, fps, clip) = decode(
            &video,
            ConversionParams {
                start_s: Some(start_s),
                fps: Some(fps),
                ..ConversionParams::for_preset(WHATSAPP_STICKER)
            },
        );
        assert_eq!(plan.frame_count(fps), expected);
        assert_eq!(
            clip.frames().len(),
            expected,
            "inicio {start_s}, {} fps",
            fps.value()
        );
        assert_eq!(
            clip.total_duration_ms(),
            (plan.output_duration_s * 1000.0).round() as u64
        );
    }
}

#[test]
fn with_factor_2_exits_half_of_duration_with_all_range() {
    let video = TestVideo::new(Content::RedThenBlue, 320, 240, 8.0);
    let (plan, _, clip) = decode(
        &video,
        ConversionParams {
            max_seconds: Some(4.0),
            automatic_fitting: true,
            ..ConversionParams::for_preset(WHATSAPP_STICKER)
        },
    );
    assert_eq!(plan.factor_speed, 2.0);
    assert_eq!(clip.frames().len(), 60);
    assert_eq!(clip.total_duration_ms(), 4000);

    let center = SIDE / 2;
    let red = clip
        .frames()
        .iter()
        .filter(|f| is_red(pixel(&f.rgba, center, center)))
        .count();
    assert!((28..=32).contains(&red), "{red} frames rojos de 60");
    let last = &clip.frames()[59].rgba;
    assert!(is_blue(pixel(last, center, center)));
}

#[test]
fn inspect_returns_duration_and_dimensions() {
    let info = FfmpegDecoder::new(TestVideo::vertical(3.0).path())
        .inspect()
        .unwrap();
    assert!((info.duration_s - 3.0).abs() < 0.1);
    assert_eq!((info.width, info.height), (360, 640));
}

#[test]
fn without_ffmpeg_nor_ffprobe_error_says_as_install_it() {
    let path = TestVideo::horizontal(3.0).path();
    let error = FfmpegDecoder::new(&path)
        .with_executables("/no/existe/ffmpeg", "/no/existe/ffprobe")
        .inspect()
        .unwrap_err();
    assert!(
        matches!(&error, DecodingError::FfmpegNotFound { program } if program.contains("ffprobe"))
    );
    let message = error.to_string();
    assert!(message.contains("instala FFmpeg"), "{message}");
    assert!(!message.contains("ruta del ejecutable"), "{message}");

    let decoder = FfmpegDecoder::new(&path);
    let (plan, fps) = plan_for(&decoder, ConversionParams::for_preset(WHATSAPP_STICKER));
    let error = decoder
        .with_executables("/no/existe/ffmpeg", "ffprobe")
        .decode(&plan, fps, SIDE, &AtomicBool::new(false))
        .unwrap_err();
    assert!(
        matches!(&error, DecodingError::FfmpegNotFound { program } if program.contains("ffmpeg"))
    );
}

#[test]
fn valid_that_input_is_file_existing_and_not_huge() {
    let directory = tempfile::tempdir().unwrap();

    let error = FfmpegDecoder::new(directory.path().join("no-existe.mp4"))
        .inspect()
        .unwrap_err();
    assert!(matches!(error, DecodingError::FileNotFound { .. }));
    assert!(error.to_string().contains("no existe el video"));

    let error = FfmpegDecoder::new(directory.path()).inspect().unwrap_err();
    assert!(matches!(error, DecodingError::NotRegularFile { .. }));

    let error = FfmpegDecoder::new(TestVideo::horizontal(3.0).path())
        .with_max_size(1000)
        .inspect()
        .unwrap_err();
    assert!(matches!(
        error,
        DecodingError::FileTooLarge {
            max_bytes: 1000,
            ..
        }
    ));
}

#[test]
fn file_that_is_not_video_returns_error_with_stderr_of_ffprobe() {
    let directory = tempfile::tempdir().unwrap();
    let text = directory.path().join("notas.mp4");
    fs::write(&text, "esto no es un video").unwrap();
    let error = FfmpegDecoder::new(&text).inspect().unwrap_err();
    match &error {
        DecodingError::FfmpegFailed {
            summarized_stderr, ..
        } => assert!(
            summarized_stderr.contains("Invalid data"),
            "{summarized_stderr}"
        ),
        other => panic!("error inesperado: {other:?}"),
    }
}

#[test]
fn file_only_of_audio_has_no_track_of_video() {
    let directory = tempfile::tempdir().unwrap();
    let audio = directory.path().join("audio.m4a");
    let state = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-f", "lavfi", "-i", "sine=duration=1", "-c:a", "aac"])
        .arg(&audio)
        .status()
        .unwrap();
    assert!(state.success());
    let error = FfmpegDecoder::new(&audio).inspect().unwrap_err();
    assert!(
        matches!(error, DecodingError::WithoutVideoTrack),
        "{error:?}"
    );
}

#[test]
fn if_exhausts_time_stops_ffmpeg_with_error_clear() {
    let decoder = FfmpegDecoder::new(TestVideo::horizontal(3.0).path());
    let (plan, fps) = plan_for(&decoder, ConversionParams::for_preset(WHATSAPP_STICKER));
    let error = decoder
        .with_time_limit(Duration::from_millis(1))
        .decode(&plan, fps, SIDE, &AtomicBool::new(false))
        .unwrap_err();
    assert!(
        matches!(&error, DecodingError::TimedOut { limit, .. } if *limit == Duration::from_millis(1)),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("se detuvo"), "{message}");
    assert!(!message.contains("tiempo límite"), "{message}");
}

#[test]
fn decode_two_times_returns_same_frames() {
    let video = TestVideo::new(Content::Noise, 640, 360, 3.0);
    let (_, _, first) = decode(&video, ConversionParams::for_preset(WHATSAPP_STICKER));
    let (_, _, second) = decode(&video, ConversionParams::for_preset(WHATSAPP_STICKER));
    assert!(first == second, "los frames difieren entre ejecuciones");
}

fn decode_image(path: impl Into<PathBuf>) -> Result<RgbaImage, DecodingError> {
    FfmpegDecoder::new(path).decode_image(&AtomicBool::new(false))
}

fn pixel_of(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let start = ((y * image.width() + x) * 4) as usize;
    image.rgba()[start..start + 4]
        .try_into()
        .expect("un píxel son 4 bytes")
}

#[test]
fn png_keeps_sides_band_center_and_alpha() {
    let path = TestImage::png(ImageContent::CenterBand, 1280, 720).path();
    let info = FfmpegDecoder::new(&path).inspect_image().unwrap();
    assert_eq!(
        info,
        ImageInfo {
            width: 1280,
            height: 720
        }
    );

    let image = decode_image(&path).unwrap();
    assert_eq!((image.width(), image.height()), (1280, 720));
    let [red, green, blue, alpha] = pixel_of(&image, 640, 360);
    assert!(red > 240 && green < 15 && blue < 15, "{red} {green} {blue}");
    assert_eq!(alpha, 255);
    for x in [0, 200, 1000, 1279] {
        assert_eq!(pixel_of(&image, x, 360)[3], 0, "x = {x}");
    }
}

#[test]
fn photo_large_decodes_with_side_smaller_1024() {
    let photo = TestImage::png(ImageContent::Pattern, 4000, 3000)
        .with_format(ImageFormat::Jpeg)
        .path();
    let info = FfmpegDecoder::new(&photo).inspect_image().unwrap();
    assert_eq!(
        info,
        ImageInfo {
            width: 4000,
            height: 3000
        }
    );
    let image = decode_image(&photo).unwrap();
    assert_eq!((image.width(), image.height()), (1365, 1024));
}

#[test]
fn panoramic_reduces_by_its_side_smaller_and_small_not_enlarges() {
    for ((width, height), expected) in [
        ((1100, 4400), (1024, 4096)),
        ((6000, 200), (6000, 200)),
        ((64, 48), (64, 48)),
    ] {
        let path = TestImage::png(ImageContent::Pattern, width, height).path();
        let image = decode_image(path).unwrap();
        assert_eq!((image.width(), image.height()), expected);
    }
}

#[test]
fn reads_png_jpeg_webp_and_bmp() {
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Bmp,
    ] {
        let path = TestImage::png(ImageContent::Pattern, 320, 180)
            .with_format(format)
            .path();
        let info = FfmpegDecoder::new(&path).inspect_image().unwrap();
        assert_eq!(
            info,
            ImageInfo {
                width: 320,
                height: 180
            },
            "{format:?}"
        );
        let image = decode_image(&path).unwrap();
        assert_eq!((image.width(), image.height()), (320, 180), "{format:?}");
    }
}

#[test]
fn of_webp_animated_reads_first_frame() {
    let (_directory, path) = ffmpeg_animation("webp", &["-c:v", "libwebp_anim"]);
    let image = decode_image(path).unwrap();
    assert_eq!((image.width(), image.height()), (64, 48));
}

#[test]
fn image_with_pixels_not_squares_decodes_as_sees() {
    let path = TestImage::png(ImageContent::Pattern, 320, 180)
        .with_sar(2, 1)
        .path();
    let image = decode_image(path).unwrap();
    assert_eq!((image.width(), image.height()), (640, 180));
}

#[test]
fn image_of_51_mb_rejects_without_read_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("enorme.png");
    fs::File::create(&path)
        .unwrap()
        .set_len(51_000_000)
        .unwrap();
    let decoder =
        FfmpegDecoder::new(&path).with_executables("no-existe-ffmpeg", "no-existe-ffprobe");
    for result in [
        decoder.inspect_image().map(|_| ()),
        decoder.decode_image(&AtomicBool::new(false)).map(|_| ()),
    ] {
        assert!(
            matches!(
                result,
                Err(DecodingError::FileTooLarge {
                    max_bytes: MAX_IMAGE_SIZE_BYTES,
                    ..
                })
            ),
            "{result:?}"
        );
    }
}

#[test]
fn that_is_not_image_returns_failure_of_ffmpeg() {
    let directory = tempfile::tempdir().unwrap();
    let neighbor = directory.path().join("vecina.png");
    fs::copy(
        TestImage::png(ImageContent::Pattern, 64, 48).path(),
        &neighbor,
    )
    .unwrap();
    for (name, content) in [
        ("texto.png", "esto no es una imagen\n"),
        ("lista.png", "ffconcat version 1.0\nfile vecina.png\n"),
    ] {
        let path = directory.path().join(name);
        fs::write(&path, content).unwrap();
        for result in [
            FfmpegDecoder::new(&path).inspect_image().map(|_| ()),
            decode_image(&path).map(|_| ()),
        ] {
            assert!(
                matches!(result, Err(DecodingError::FfmpegFailed { .. })),
                "{name}: {result:?}"
            );
        }
    }
    let video = decode_image(TestVideo::horizontal(3.0).path());
    assert!(
        matches!(video, Err(DecodingError::FfmpegFailed { .. })),
        "{video:?}"
    );
}

#[test]
fn image_truncated_returns_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("truncada.png");
    let whole = fs::read(TestImage::png(ImageContent::Pattern, 320, 180).path()).unwrap();
    fs::write(&path, &whole[..50]).unwrap();
    assert!(decode_image(&path).is_err());
}

#[test]
fn image_over_16384_px_rejects_before_of_decode_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ancha.png");
    let mut encoder = png::Encoder::new(fs::File::create(&path).unwrap(), 20_000, 2);
    encoder.set_color(png::ColorType::Grayscale);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[0; 40_000])
        .unwrap();
    let decoder = FfmpegDecoder::new(&path);
    for result in [
        decoder.inspect_image().map(|_| ()),
        decoder
            .clone()
            .with_executables("no-existe-ffmpeg", "ffprobe")
            .decode_image(&AtomicBool::new(false))
            .map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(DecodingError::DimensionsNotValid { .. })),
            "{result:?}"
        );
    }
}

#[test]
fn image_with_dash_protocol_or_pattern_in_name_is_own_file() {
    let directory = tempfile::tempdir().unwrap();
    let own = TestImage::png(ImageContent::Pattern, 320, 180).path();
    let other = TestImage::png(ImageContent::Pattern, 64, 48).path();
    for decoy in ["x.png", "a.png", "foto000.png"] {
        fs::copy(&other, directory.path().join(decoy)).unwrap();
    }
    for name in ["-x.png", "concat:a.png", "foto%03d.png", "sin_extension"] {
        let path = directory.path().join(name);
        fs::copy(&own, &path).unwrap();
        let info = FfmpegDecoder::new(&path).inspect_image().unwrap();
        assert_eq!(
            info,
            ImageInfo {
                width: 320,
                height: 180
            },
            "{name}"
        );
        let image = decode_image(&path).unwrap();
        assert_eq!((image.width(), image.height()), (320, 180), "{name}");
    }
}

#[test]
fn decode_image_can_cancel() {
    let path = TestImage::png(ImageContent::Pattern, 64, 48).path();
    let result = FfmpegDecoder::new(path).decode_image(&AtomicBool::new(true));
    assert!(
        matches!(result, Err(DecodingError::Cancelled)),
        "{result:?}"
    );
}

#[test]
fn image_that_does_not_exist_or_is_directory_returns_its_error() {
    let directory = tempfile::tempdir().unwrap();
    assert!(matches!(
        decode_image(directory.path().join("no.png")),
        Err(DecodingError::FileNotFound { .. })
    ));
    assert!(matches!(
        decode_image(directory.path()),
        Err(DecodingError::NotRegularFile { .. })
    ));
}

#[cfg(unix)]
#[test]
fn cancel_inspection_kills_ffprobe_right_away() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("ffprobe.sh");
    let file_pid = directory.path().join("pid");
    fs::write(
        &source,
        format!(
            "#!/bin/sh\necho $$ > '{0}.tmp'\nmv '{0}.tmp' '{0}'\nexec sleep 30\n",
            file_pid.display()
        ),
    )
    .unwrap();
    let slow_ffprobe = directory.path().join("ffprobe");
    let state = Command::new("install")
        .args(["-m", "755"])
        .arg(&source)
        .arg(&slow_ffprobe)
        .status()
        .unwrap();
    assert!(state.success(), "install falló");

    type Inspection = fn(&FfmpegDecoder, &AtomicBool) -> Result<(), DecodingError>;
    let video: Inspection =
        |decoder, cancellation| decoder.inspect_cancelable(cancellation).map(|_| ());
    let image: Inspection =
        |decoder, cancellation| decoder.inspect_image_cancelable(cancellation).map(|_| ());
    for (name, path, inspection) in [
        ("video", TestVideo::horizontal(3.0).path(), video),
        (
            "imagen",
            TestImage::png(ImageContent::Pattern, 64, 48).path(),
            image,
        ),
    ] {
        let decoder = FfmpegDecoder::new(path).with_executables("ffmpeg", &slow_ffprobe);
        let cancellation = AtomicBool::new(false);
        let (result, pid, waits) = std::thread::scope(|scope| {
            let inspecting = scope.spawn(|| inspection(&decoder, &cancellation));
            let started = std::time::Instant::now();
            while !file_pid.is_file() && started.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(10));
            }
            cancellation.store(true, std::sync::atomic::Ordering::Relaxed);
            let cancelled = std::time::Instant::now();
            let result = inspecting.join().unwrap();
            let pid = fs::read_to_string(&file_pid).expect("el ffprobe falso arrancó");
            (result, pid, cancelled.elapsed())
        });
        assert!(
            matches!(result, Err(DecodingError::Cancelled)),
            "{name}: {result:?}"
        );
        assert!(waits < Duration::from_secs(2), "{name}: tardó {waits:?}");
        let alive = Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive, "{name}: ffprobe sigue vivo");
        fs::remove_file(&file_pid).unwrap();
    }
}

#[test]
fn inspect_without_cancel_returns_same_that_inspect() {
    let without_cancel = AtomicBool::new(false);
    let video = FfmpegDecoder::new(TestVideo::horizontal(3.0).path());
    assert_eq!(
        video.inspect_cancelable(&without_cancel).unwrap(),
        video.inspect().unwrap()
    );
    let image = FfmpegDecoder::new(TestImage::png(ImageContent::Pattern, 64, 48).path());
    assert_eq!(
        image.inspect_image_cancelable(&without_cancel).unwrap(),
        image.inspect_image().unwrap()
    );
}

fn generate_video(path: &std::path::Path, arguments: &[&str]) {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-f", "lavfi", "-i", "testsrc2=size=160x120:rate=25"])
        .args(["-t", "2"])
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

#[test]
fn video_trap_does_not_read_others_files() {
    let directory = tempfile::tempdir().unwrap();
    let in_directory = |name: &str| directory.path().join(name);
    generate_video(&in_directory("secreto.mp4"), &["-c:v", "mpeg4"]);
    generate_video(&in_directory("secreto.ts"), &["-c:v", "mpeg2video"]);
    generate_video(
        &in_directory("foto%03d.png"),
        &["-f", "image2", "-frames:v", "25"],
    );
    let sequence = in_directory("foto%03d.png");
    fs::copy(in_directory("foto001.png"), &sequence).unwrap();

    let concat = "ffconcat version 1.0\nfile secreto.mp4\nduration 2\n";
    let concat_gif = "ffconcat version 1.0\nfile secreto.ts\nduration 2\n";
    let hls = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:2\n#EXTINF:2.0,\nsecreto.ts\n\
               #EXT-X-ENDLIST\n";
    let dash = "<?xml version=\"1.0\"?><MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\" type=\"static\" \
                mediaPresentationDuration=\"PT2S\"><Period><AdaptationSet mimeType=\"video/mp4\">\
                <Representation id=\"0\" bandwidth=\"1000\" width=\"160\" height=\"120\">\
                <BaseURL>secreto.mp4</BaseURL></Representation></AdaptationSet></Period></MPD>\n";
    let mut traps = vec![sequence];
    for (name, content) in [
        ("inocente.mp4", concat),
        ("inocente.gif", concat_gif),
        ("lista.ffconcat", concat),
        ("lista.m3u8", hls),
        ("inocente.ts", hls),
        ("manifiesto.mpd", dash),
    ] {
        fs::write(in_directory(name), content).unwrap();
        traps.push(in_directory(name));
    }

    let plan = TimePlan {
        start_s: 0.0,
        input_duration_s: 1.0,
        factor_speed: 1.0,
        output_duration_s: 1.0,
    };
    let mut read = Vec::new();
    for trap in &traps {
        let decoder = FfmpegDecoder::new(trap);
        let name = trap.file_name().unwrap().to_string_lossy().into_owned();
        match decoder.inspect() {
            Err(DecodingError::FfmpegFailed { .. }) => {}
            other => read.push(format!("{name}, al inspeccionar: {other:?}")),
        }
        let cancellation = AtomicBool::new(false);
        match decoder.decode(&plan, Fps::DEFAULT, 64, &cancellation) {
            Err(DecodingError::FfmpegFailed { .. }) => {}
            Err(other) => read.push(format!("{name}, al decodificar: {other:?}")),
            Ok(_) => read.push(format!("{name}, al decodificar: dio frames")),
        }
    }
    assert!(read.is_empty(), "{read:#?}");
}

#[test]
fn follow_entering_containers_of_video_usual() {
    let directory = tempfile::tempdir().unwrap();
    let cases: [(&str, &[&str]); 17] = [
        ("v.mp4", &["-c:v", "mpeg4"]),
        (
            "fragmentado.mp4",
            &["-c:v", "mpeg4", "-movflags", "frag_keyframe+empty_moov"],
        ),
        ("v.m4v", &["-c:v", "mpeg4", "-f", "mp4"]),
        ("v.mov", &["-c:v", "mpeg4"]),
        ("v.3gp", &["-c:v", "mpeg4"]),
        ("v.f4v", &["-c:v", "mpeg4", "-f", "mp4"]),
        ("v.mkv", &["-c:v", "mpeg4"]),
        ("v.webm", &["-c:v", "libvpx", "-deadline", "realtime"]),
        ("v.avi", &["-c:v", "mpeg4"]),
        ("v.gif", &[]),
        ("v.flv", &["-c:v", "flv1"]),
        ("v.ts", &["-c:v", "mpeg2video"]),
        ("v.m2ts", &["-c:v", "mpeg2video"]),
        ("v.mpg", &["-c:v", "mpeg2video"]),
        ("v.vob", &["-c:v", "mpeg2video"]),
        ("v.ogv", &["-c:v", "libtheora"]),
        ("v.wmv", &["-c:v", "wmv2"]),
    ];
    let mut paths = Vec::new();
    for (name, arguments) in cases {
        let path = directory.path().join(name);
        generate_video(&path, arguments);
        paths.push(path);
    }
    for (origin, name) in [
        ("v.mp4", "sin_extension"),
        ("v.mp4", "clip%03d.mp4"),
        ("v.gif", "era_un_gif.mp4"),
        ("v.ts", "video.unknown_video"),
    ] {
        let path = directory.path().join(name);
        fs::copy(directory.path().join(origin), &path).unwrap();
        paths.push(path);
    }
    for path in paths {
        let decoder = FfmpegDecoder::new(&path);
        let name = path.display();
        let info = decoder
            .inspect()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!((info.duration_s - 2.0).abs() < 0.3, "{name}: {info:?}");
        let options =
            ConversionOptions::new(&ConversionParams::for_preset(WHATSAPP_STICKER)).unwrap();
        let plan = plan_time(info.duration_s, &options).unwrap();
        let clip = decoder
            .decode(&plan, Fps::DEFAULT, 64, &AtomicBool::new(false))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(clip.frames().len() > 10, "{name}");
    }
}

fn decode_framed(framing: FramingSpec) -> Vec<u8> {
    let video = TestVideo::new(Content::Halves, 640, 360, 1.0);
    let decoder = FfmpegDecoder::new(video.path()).with_framing(framing);
    let (plan, fps) = plan_for(&decoder, ConversionParams::for_preset(WHATSAPP_STICKER));
    let clip = decoder
        .decode(&plan, fps, SIDE, &AtomicBool::new(false))
        .expect("el video se decodifica");
    clip.frames()[0].rgba.to_vec()
}

#[test]
fn crop_off_centre_picks_the_requested_region() {
    let rgba = decode_framed(FramingSpec::default());
    assert!(is_red(pixel(&rgba, 100, 256)) && is_blue(pixel(&rgba, 400, 256)));

    let left = FramingSpec::new(FramingMode::Crop, 0.0, 0.5, 1.0).unwrap();
    let rgba = decode_framed(left);
    assert!(is_red(pixel(&rgba, 100, 256)) && is_red(pixel(&rgba, 400, 256)));
    assert!(is_blue(pixel(&rgba, 500, 256)));

    let right = FramingSpec::new(FramingMode::Crop, 1.0, 0.5, 1.0).unwrap();
    let rgba = decode_framed(right);
    assert!(is_red(pixel(&rgba, 10, 256)) && is_blue(pixel(&rgba, 100, 256)));
}

#[test]
fn zoom_2_crops_a_quarter_of_the_square() {
    let zoom = FramingSpec::new(FramingMode::Crop, 0.25, 0.5, 2.0).unwrap();
    let rgba = decode_framed(zoom);
    for (x, y) in [(10, 10), (256, 256), (500, 500)] {
        assert!(is_red(pixel(&rgba, x, y)), "({x}, {y})");
    }
}

#[test]
fn fit_keeps_the_whole_frame_with_transparent_bars() {
    let rgba = decode_framed(FramingSpec::fit());
    for (x, y) in [(256, 10), (10, 500), (500, 50)] {
        assert_eq!(pixel(&rgba, x, y)[3], 0, "({x}, {y})");
    }
    assert!(is_red(pixel(&rgba, 10, 256)) && is_blue(pixel(&rgba, 500, 256)));
}

#[test]
fn frame_at_returns_a_covering_square_of_raw_pixels() {
    let video = TestVideo::new(Content::RedThenBlue, 320, 180, 2.0);
    let decoder = FfmpegDecoder::new(video.path());
    let cancellation = AtomicBool::new(false);
    let side = 64;

    let first = decoder.frame_at(0.3, side, &cancellation).unwrap();
    let last = decoder.frame_at(1.7, side, &cancellation).unwrap();

    assert_eq!(first.len(), (side * side * 4) as usize);
    assert!(is_red([first[0], first[1], first[2], first[3]]));
    let centre = ((side / 2 * side + side / 2) * 4) as usize;
    assert!(is_blue([
        last[centre],
        last[centre + 1],
        last[centre + 2],
        last[centre + 3]
    ]));
    let webp = conversor_core::encode_thumbnail_webp(&first, side).unwrap();
    assert!(webp.starts_with(b"RIFF") && webp.len() < 2_000);
}

#[test]
fn frame_at_past_the_end_is_an_error_not_an_empty_frame() {
    let video = TestVideo::new(Content::RedThenBlue, 320, 180, 1.0);
    let decoder = FfmpegDecoder::new(video.path());
    let error = decoder
        .frame_at(50.0, 64, &AtomicBool::new(false))
        .unwrap_err();
    assert!(
        matches!(error, DecodingError::VideoWithoutFrames),
        "{error}"
    );
}
