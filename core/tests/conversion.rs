#![allow(clippy::panic, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use conversor_core::{
    Clip, ConversionError, ConversionOptions, ConversionParams, DISCORD_GIF, Decoder,
    DecodingError, FfmpegDecoder, Fps, Preset, Priority, Progress, Report, Speed, TimePlan,
    Tracking, VideoInfo, WHATSAPP_STICKER, convert, estimate, plan,
};
use tempfile::TempDir;

use common::{Animation, Content, TestVideo, read_gif, read_webp};

fn options(preset: Preset) -> ConversionOptions {
    ConversionOptions::new(&ConversionParams::for_preset(preset)).expect("opciones válidas")
}

fn convert_video(video: &TestVideo, preset: Preset) -> (TempDir, Report, Vec<u8>) {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let output = directory
        .path()
        .join(format!("salida.{}", preset.format.extension()));
    let report = convert(
        &FfmpegDecoder::new(video.path()),
        &options(preset),
        Tracking::none(),
    )
    .unwrap_or_else(|error| panic!("{preset} sobre {}: {error}", video.file_name()));
    fs::write(&output, &report.bytes).expect("se puede escribir la salida");
    let bytes = report.bytes.clone();
    (directory, report, bytes)
}

fn convert_with(video: &TestVideo, options: &ConversionOptions) -> Report {
    convert(&FfmpegDecoder::new(video.path()), options, Tracking::none())
        .unwrap_or_else(|error| panic!("{} sobre {}: {error}", options.preset(), video.file_name()))
}

fn only_output(directory: &Path) -> Vec<String> {
    fs::read_dir(directory)
        .expect("se puede listar el directorio")
        .map(|input| {
            input
                .expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn check_common(animation: &Animation, report: &Report, tolerance_ms: u64) {
    assert!(animation.frame_count() > 0);
    assert!(animation.size_bytes <= report.max_size.bytes());
    assert_eq!(animation.size_bytes, report.size_bytes());
    assert_eq!(animation.width, report.canvas_side);
    assert_eq!(animation.height, report.canvas_side);
    let total = animation.total_duration_ms();
    assert!(
        total.abs_diff(report.duration_ms) <= tolerance_ms,
        "archivo {total} ms, informe {} ms",
        report.duration_ms
    );
    assert!(report.duration_ms as f64 <= report.plan.output_duration_s * 1000.0 + 1.0);
}

#[test]
fn video_of_3_s_returns_whatsapp_sticker_valid() {
    let video = TestVideo::horizontal(3.0);
    let (directory, report, _) = convert_video(&video, WHATSAPP_STICKER);
    let animation = read_webp(&directory.path().join("salida.webp"));

    assert_eq!((animation.width, animation.height), (512, 512));
    assert!(
        animation.size_bytes <= 500_000,
        "{} B",
        animation.size_bytes
    );
    assert!(animation.total_duration_ms() <= 10_000);
    assert!(animation.min_duration_ms().is_some_and(|ms| ms >= 8));
    assert!((2_900..=3_000).contains(&animation.total_duration_ms()));
    check_common(&animation, &report, 0);
    assert_eq!(report.preset, "whatsapp-sticker");
    assert_eq!(only_output(directory.path()), ["salida.webp"]);
}

#[test]
fn video_of_3_s_returns_discord_gif_valid() {
    let video = TestVideo::vertical(3.0);
    let (directory, report, _) = convert_video(&video, DISCORD_GIF);
    let animation = read_gif(&directory.path().join("salida.gif"));

    assert_eq!(animation.width, animation.height);
    assert!(animation.width <= 480);
    assert!(animation.size_bytes <= 10_000_000);
    assert!(animation.total_duration_ms() <= 30_000);
    check_common(&animation, &report, 10);
    assert_eq!(report.preset, "discord-gif");
    assert_eq!(only_output(directory.path()), ["salida.gif"]);
}

#[test]
fn convert_two_times_returns_same_bytes() {
    let video = TestVideo::horizontal(3.0);
    for preset in [WHATSAPP_STICKER, DISCORD_GIF] {
        let (_, first_report, first) = convert_video(&video, preset);
        let (_, second_report, second) = convert_video(&video, preset);
        assert!(first == second, "{preset}: los bytes difieren");
        assert!(
            first_report == second_report,
            "{preset}: los informes difieren"
        );
    }
}

#[test]
fn with_thread_returns_same_bytes_that_with_pool_default() {
    let video = TestVideo::horizontal(3.0);
    let thread = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("pool de 1 hilo");
    for preset in [WHATSAPP_STICKER, DISCORD_GIF] {
        let (_, _, default) = convert_video(&video, preset);
        let (_, _, sequential) = thread.install(|| convert_video(&video, preset));
        assert!(default == sequential, "{preset}: los bytes difieren");
    }
}

#[test]
fn with_thread_returns_same_bytes_with_sample_in_windows_and_ladder() {
    let video = TestVideo::new(Content::Noise, 48, 28, 3.5);
    let thread = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("pool de 1 hilo");
    for preset in [WHATSAPP_STICKER, DISCORD_GIF] {
        let options = ConversionOptions::new(&ConversionParams {
            max_mb: preset.range_max_mb.map(|_| 2.0),
            ..ConversionParams::for_preset(preset)
        })
        .expect("opciones válidas");
        let default = convert_with(&video, &options);
        let sequential = thread.install(|| convert_with(&video, &options));
        assert!(
            default.plan.frame_count(options.fps()) > 45,
            "{preset}: el clip no pasa por la muestra en ventanas"
        );
        assert!(
            !default.reductions.is_empty(),
            "{preset}: el ruido no recorrió la escalera"
        );
        assert!(
            default.bytes == sequential.bytes,
            "{preset}: los bytes difieren"
        );
        assert!(default == sequential, "{preset}: los informes difieren");
    }
}

#[test]
fn reports_of_stages_in_order() {
    let stages = Mutex::new(Vec::new());
    let progress = |stage| stages.lock().expect("sin envenenar").push(stage);
    let cancellation = AtomicBool::new(false);
    convert(
        &FfmpegDecoder::new(TestVideo::horizontal(3.0).path()),
        &options(WHATSAPP_STICKER),
        Tracking::new(&progress, &cancellation),
    )
    .expect("convierte");
    let stages = stages.into_inner().expect("sin envenenar");
    assert_eq!(stages.first(), Some(&Progress::Decoding));
    assert!(stages.len() > 1, "{stages:?}");
    assert!(
        stages[1..].iter().all(|stage| matches!(
            stage,
            Progress::FittingSize {
                attempt: 1..,
                max: 10
            }
        )),
        "{stages:?}"
    );
}

#[test]
fn cancelled_before_of_start_delivers_nothing() {
    let cancellation = AtomicBool::new(true);
    let stages = Mutex::new(Vec::new());
    let progress = |stage| stages.lock().expect("sin envenenar").push(stage);
    let deliveries = Mutex::new(0);
    let receive = |_: &[u8]| {
        *deliveries.lock().expect("sin envenenar") += 1;
        Ok(())
    };
    let options = ConversionOptions::new(&ConversionParams {
        preview: true,
        ..ConversionParams::for_preset(WHATSAPP_STICKER)
    })
    .expect("opciones válidas");
    let error = convert(
        &FfmpegDecoder::new(TestVideo::horizontal(3.0).path()),
        &options,
        Tracking::new(&progress, &cancellation).with_preview(&receive),
    )
    .unwrap_err();
    assert!(matches!(error, ConversionError::Cancelled), "{error:?}");
    assert!(stages.into_inner().expect("sin envenenar").is_empty());
    assert_eq!(deliveries.into_inner().expect("sin envenenar"), 0);
}

#[test]
fn cancel_while_decodes_stops_ffmpeg_right_away() {
    let decoder = FfmpegDecoder::new(TestVideo::horizontal(25.0).path());
    let options = ConversionOptions::new(&ConversionParams {
        max_seconds: Some(30.0),
        fps: Some(30),
        ..ConversionParams::for_preset(DISCORD_GIF)
    })
    .expect("opciones válidas");
    let plan = plan(&decoder, &options).expect("el tramo entra");
    let cancellation = AtomicBool::new(false);
    let start = Instant::now();
    let result = thread::scope(|threads| {
        threads.spawn(|| {
            thread::sleep(Duration::from_millis(100));
            cancellation.store(true, Ordering::Relaxed);
        });
        decoder.decode(&plan, options.fps(), 480, &cancellation)
    });
    let error = result.unwrap_err();
    assert!(matches!(error, DecodingError::Cancelled), "{error:?}");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "tardó {:?} en detenerse",
        start.elapsed()
    );
}

struct InterruptedFfprobe<'a> {
    cancellation: &'a AtomicBool,
}

impl Decoder for InterruptedFfprobe<'_> {
    fn inspect(&self) -> Result<VideoInfo, DecodingError> {
        self.cancellation.store(true, Ordering::Relaxed);
        Err(DecodingError::FfmpegFailed {
            program: "ffprobe".to_string(),
            code: None,
            summarized_stderr: "terminado por una señal".to_string(),
        })
    }

    fn decode(&self, _: &TimePlan, _: Fps, _: u32, _: &AtomicBool) -> Result<Clip, DecodingError> {
        panic!("no debería decodificar tras una inspección fallida")
    }
}

#[test]
fn failure_of_ffprobe_with_cancellation_active_is_cancelled() {
    let cancellation = AtomicBool::new(false);
    let error = convert(
        &InterruptedFfprobe {
            cancellation: &cancellation,
        },
        &options(WHATSAPP_STICKER),
        Tracking::new(&|_| {}, &cancellation),
    )
    .unwrap_err();
    assert!(matches!(error, ConversionError::Cancelled), "{error:?}");
}

#[test]
fn cancellation_of_decoding_arrives_as_cancelled() {
    let error: ConversionError = DecodingError::Cancelled.into();
    assert!(matches!(error, ConversionError::Cancelled));
}

#[derive(Debug)]
enum Event {
    Stage(Progress),
    Preview(Vec<u8>),
}

#[test]
fn preview_delivers_before_of_fit_size_and_sample_acceleration() {
    let video = TestVideo::horizontal(4.0);
    let options = ConversionOptions::new(&ConversionParams {
        max_seconds: Some(2.0),
        automatic_fitting: true,
        preview: true,
        ..ConversionParams::for_preset(WHATSAPP_STICKER)
    })
    .expect("opciones válidas");
    let events = Mutex::new(Vec::new());
    let progress = |stage| {
        events
            .lock()
            .expect("sin envenenar")
            .push(Event::Stage(stage));
    };
    let receive = |bytes: &[u8]| {
        events
            .lock()
            .expect("sin envenenar")
            .push(Event::Preview(bytes.to_vec()));
        Ok(())
    };
    let cancellation = AtomicBool::new(false);
    let report = convert(
        &FfmpegDecoder::new(video.path()),
        &options,
        Tracking::new(&progress, &cancellation).with_preview(&receive),
    )
    .expect("convierte");

    let events = events.into_inner().expect("sin envenenar");
    let position = |searched: fn(&Event) -> bool| {
        events
            .iter()
            .position(searched)
            .unwrap_or_else(|| panic!("falta un evento en {} eventos", events.len()))
    };
    let delivered = position(|e| matches!(e, Event::Preview(_)));
    assert!(position(|e| matches!(e, Event::Stage(Progress::Preview))) < delivered);
    assert!(delivered < position(|e| matches!(e, Event::Stage(Progress::FittingSize { .. }))));
    let Event::Preview(received) = &events[delivered] else {
        unreachable!("la posición es la de la vista previa");
    };
    assert!(report.preview.as_ref() == Some(received));

    assert_eq!(report.plan.factor_speed, 2.0);
    let directory = tempfile::tempdir().expect("directorio temporal");
    let preview_path = directory.path().join("gato.preview.webp");
    fs::write(&preview_path, received).expect("se puede escribir la vista previa");
    let preview = read_webp(&preview_path);
    assert_eq!((preview.width, preview.height), (160, 160));
    let half_ms = (report.plan.input_duration_s * 1000.0 / 2.0) as u64;
    assert!(
        half_ms.abs_diff(preview.total_duration_ms()) < 67,
        "vista previa de {} ms, mitad del tramo {half_ms} ms",
        preview.total_duration_ms()
    );
}

#[test]
fn if_not_can_save_preview_conversion_stops() {
    let options = ConversionOptions::new(&ConversionParams {
        preview: true,
        ..ConversionParams::for_preset(WHATSAPP_STICKER)
    })
    .expect("opciones válidas");
    let stages = Mutex::new(Vec::new());
    let progress = |stage| stages.lock().expect("sin envenenar").push(stage);
    let fail = |_: &[u8]| Err(std::io::Error::other("disco lleno"));
    let cancellation = AtomicBool::new(false);
    let error = convert(
        &FfmpegDecoder::new(TestVideo::horizontal(3.0).path()),
        &options,
        Tracking::new(&progress, &cancellation).with_preview(&fail),
    )
    .unwrap_err();
    assert!(
        matches!(error, ConversionError::PreviewNotSaved(_)),
        "{error:?}"
    );
    assert!(error.to_string().contains("disco lleno"), "{error}");
    let stages = stages.into_inner().expect("sin envenenar");
    assert!(
        !stages
            .iter()
            .any(|e| matches!(e, Progress::FittingSize { .. })),
        "{stages:?}"
    );
}

#[test]
fn priorities_meet_preset_and_not_depend_of_threads() {
    let video = TestVideo::new(Content::Pattern, 320, 180, 3.0);
    for (priority, qualities) in [
        (Priority::Sharpness, 75..=90),
        (Priority::Smoothness, 35..=85),
    ] {
        let options = ConversionOptions::new(&ConversionParams {
            fps: Some(priority.default_fps().value()),
            ..ConversionParams::for_preset(WHATSAPP_STICKER)
        })
        .expect("opciones válidas")
        .with_priority(priority);
        let report = convert_with(&video, &options);
        assert!(report.size_bytes() <= 500_000, "{priority:?}");
        assert!(
            qualities.contains(&report.attempt.value),
            "{priority:?}: {}",
            report.attempt
        );
        let thread = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("un grupo de un hilo")
            .install(|| convert_with(&video, &options));
        assert_eq!(thread, report, "{priority:?}");
    }
}

#[test]
fn that_estimate_says_that_fits_fits_at_the_convert() {
    let video = TestVideo::horizontal(3.0);
    let decoder = FfmpegDecoder::new(video.path());
    let options = options(WHATSAPP_STICKER);
    let stages = Mutex::new(Vec::new());
    let progress = |stage| stages.lock().unwrap().push(stage);
    let cancellation = AtomicBool::new(false);
    let estimate = estimate(&decoder, &options, Tracking::new(&progress, &cancellation))
        .expect("se puede estimar");
    assert!(estimate.fits, "{estimate:?}");
    assert!(estimate.estimated_size_bytes <= 475_000, "{estimate:?}");
    assert_eq!(estimate.max_size, options.max_size());
    assert_eq!(estimate.plan.output_duration_s, 3.0);
    assert!((3.0..=10.0).contains(&estimate.max_seconds_s));
    assert_eq!(
        *stages.lock().unwrap(),
        [
            Progress::Decoding,
            Progress::FittingSize {
                attempt: 1,
                max: 10
            }
        ]
    );

    let report = convert_with(&video, &options);
    assert!(report.size_bytes() <= options.max_size().bytes());
}

#[test]
fn estimate_says_that_does_not_fit_and_how_many_seconds_fit() {
    let video = TestVideo::new(Content::Noise, 640, 360, 8.0);
    let options = ConversionOptions::new(&ConversionParams {
        max_seconds: Some(8.0),
        ..ConversionParams::for_preset(WHATSAPP_STICKER)
    })
    .expect("opciones válidas");
    let estimate = estimate(
        &FfmpegDecoder::new(video.path()),
        &options,
        Tracking::none(),
    )
    .expect("se puede estimar");
    assert!(!estimate.fits, "{estimate:?}");
    assert!(estimate.estimated_size_bytes > 475_000, "{estimate:?}");
    assert!(estimate.max_seconds_s < 8.0, "{estimate:?}");
}

#[test]
fn estimate_cancels_a_half() {
    let video = TestVideo::horizontal(3.0);
    let cancellation = AtomicBool::new(false);
    let progress = |_| cancellation.store(true, Ordering::Relaxed);
    let result = estimate(
        &FfmpegDecoder::new(video.path()),
        &options(WHATSAPP_STICKER),
        Tracking::new(&progress, &cancellation),
    );
    assert!(
        matches!(result, Err(ConversionError::Cancelled)),
        "{result:?}"
    );
}

#[cfg(unix)]
fn slow_ffprobe(directory: &Path) -> std::path::PathBuf {
    let source = directory.join("ffprobe.sh");
    let pid = directory.join("pid");
    fs::write(
        &source,
        format!(
            "#!/bin/sh\necho $$ > '{0}.tmp'\nmv '{0}.tmp' '{0}'\nexec sleep 30\n",
            pid.display()
        ),
    )
    .expect("se puede escribir el script");
    let executable = directory.join("ffprobe");
    let state = std::process::Command::new("install")
        .args(["-m", "755"])
        .arg(&source)
        .arg(&executable)
        .status()
        .expect("se puede ejecutar install");
    assert!(state.success(), "install falló");
    executable
}

#[cfg(unix)]
#[test]
fn cancel_convert_or_estimate_while_inspects_kills_ffprobe() {
    type Operation = fn(&FfmpegDecoder, Tracking<'_>) -> Result<(), ConversionError>;
    let convert: Operation =
        |decoder, tracking| convert(decoder, &options(WHATSAPP_STICKER), tracking).map(|_| ());
    let estimate: Operation =
        |decoder, tracking| estimate(decoder, &options(WHATSAPP_STICKER), tracking).map(|_| ());
    for (name, operation) in [("convertir", convert), ("estimar", estimate)] {
        let directory = tempfile::tempdir().expect("directorio temporal");
        let file_pid = directory.path().join("pid");
        let decoder = FfmpegDecoder::new(TestVideo::horizontal(3.0).path())
            .with_executables("ffmpeg", slow_ffprobe(directory.path()));
        let cancellation = AtomicBool::new(false);
        let (result, waits) = thread::scope(|scope| {
            let working =
                scope.spawn(|| operation(&decoder, Tracking::new(&|_| {}, &cancellation)));
            let started = Instant::now();
            while !file_pid.is_file() && started.elapsed() < Duration::from_secs(10) {
                thread::sleep(Duration::from_millis(10));
            }
            cancellation.store(true, Ordering::Relaxed);
            let cancelled = Instant::now();
            let result = working.join().expect("sin pánico");
            (result, cancelled.elapsed())
        });
        assert!(
            matches!(result, Err(ConversionError::Cancelled)),
            "{name}: {result:?}"
        );
        assert!(waits < Duration::from_secs(2), "{name}: tardó {waits:?}");
        let pid = fs::read_to_string(&file_pid).expect("el ffprobe falso arrancó");
        let alive = std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .expect("se puede ejecutar kill")
            .success();
        assert!(!alive, "{name}: ffprobe sigue vivo");
    }
}

#[test]
fn fit_anyway_fits_what_smoothness_cannot() {
    let video = TestVideo::mandelbrot(640, 360, 6.0);
    let with = |priority: Priority| {
        let options = ConversionOptions::new(&ConversionParams {
            fps: Some(priority.default_fps().value()),
            max_seconds: Some(6.0),
            ..ConversionParams::for_preset(WHATSAPP_STICKER)
        })
        .expect("opciones válidas")
        .with_priority(priority);
        common::convert_video_with(&video, &options)
    };
    assert!(matches!(
        with(Priority::Smoothness),
        Err(ConversionError::DoesNotFitInSize { .. })
    ));
    let estimate_with = |priority: Priority| {
        let options = ConversionOptions::new(&ConversionParams::for_preset(WHATSAPP_STICKER))
            .expect("opciones válidas")
            .with_priority(priority);
        estimate(
            &FfmpegDecoder::new(video.path()),
            &options,
            Tracking::none(),
        )
        .expect("se puede estimar")
    };
    assert!(!estimate_with(Priority::Smoothness).fits);
    let squeezed = estimate_with(Priority::FitAnyway);
    assert!(squeezed.fits, "{squeezed:?}");
    assert!(squeezed.attempt.squeeze, "{squeezed:?}");
    let conversion = with(Priority::FitAnyway).expect("con FitAnyway debe caber");
    common::check_rules(
        &WHATSAPP_STICKER,
        WHATSAPP_STICKER.max_size.bytes(),
        6.0,
        &conversion.report,
        &conversion.path,
    );
}

#[test]
fn fluid_keeps_the_requested_fps_and_still_passes_the_rules() {
    let video = TestVideo::mandelbrot(640, 360, 6.0);
    let options = ConversionOptions::new(&ConversionParams {
        fps: Some(24),
        max_seconds: Some(6.0),
        ..ConversionParams::for_preset(WHATSAPP_STICKER)
    })
    .expect("opciones válidas")
    .with_priority(Priority::Fluid);
    let estimated = estimate(
        &FfmpegDecoder::new(video.path()),
        &options,
        Tracking::none(),
    )
    .expect("se puede estimar");
    assert!(estimated.fits, "{estimated:?}");
    assert_eq!(estimated.attempt.fps.value(), 24);
    let conversion = common::convert_video_with(&video, &options).expect("con Fluid debe caber");
    assert_eq!(conversion.report.attempt.fps.value(), 24);
    assert!(
        conversion
            .report
            .reductions
            .iter()
            .all(|reduction| !matches!(reduction, conversor_core::Reduction::Fps { .. }))
    );
    common::check_rules(
        &WHATSAPP_STICKER,
        WHATSAPP_STICKER.max_size.bytes(),
        6.0,
        &conversion.report,
        &conversion.path,
    );
}

#[test]
fn estimate_that_fits_matches_the_size_of_the_conversion_of_a_clip_hard_to_sample() {
    let video = TestVideo::new(Content::NoisyPattern, 960, 720, 6.0);
    let options = ConversionOptions::new(&ConversionParams {
        max_seconds: Some(6.0),
        start_s: Some(0.0),
        fps: Some(15),
        ..ConversionParams::for_preset(WHATSAPP_STICKER)
    })
    .expect("valid options")
    .with_speed(Speed::new(1.0).expect("valid speed"))
    .expect("speed without automatic fitting")
    .with_priority(Priority::Balance);
    let decoder = FfmpegDecoder::new(video.path());
    let estimate = estimate(&decoder, &options, Tracking::none()).expect("can estimate");
    let report = convert_with(&video, &options);
    assert!(estimate.fits, "{estimate:?}");
    assert!(report.size_bytes() <= options.max_size().bytes());
    let (estimated, real) = (
        estimate.estimated_size_bytes as f64,
        report.size_bytes() as f64,
    );
    assert!((estimated - real).abs() / real < 0.02, "{estimated} {real}");
}
