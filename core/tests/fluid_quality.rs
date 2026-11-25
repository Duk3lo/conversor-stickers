#![allow(clippy::panic, clippy::expect_used, clippy::print_stderr)]

//! Slow benchmark of what `fluid` achieves on hard 960x720 clips sped up 2x to 5.3 s at 24 fps.
//! Run on demand: `cargo test --release --test fluid_quality -- --ignored --nocapture`.

mod common;

use std::time::Instant;

use conversor_core::{
    ConversionOptions, ConversionParams, Priority, Ratio, Report, WHATSAPP_STICKER,
};

use common::{Content, TestVideo};

const OUTPUT_S: f64 = 5.3;

fn fluid(content: Content) -> (Report, f64) {
    let video = TestVideo::new(content, 960, 720, 2.0 * OUTPUT_S);
    let options = ConversionOptions::new(&ConversionParams {
        fps: Some(24),
        max_seconds: Some(OUTPUT_S),
        ..ConversionParams::for_preset(WHATSAPP_STICKER)
    })
    .expect("valid options")
    .with_priority(Priority::Fluid);
    let started = Instant::now();
    let conversion = common::convert_video_with(&video, &options).expect("fluid must fit");
    let seconds = started.elapsed().as_secs_f64();
    common::check_rules(
        &WHATSAPP_STICKER,
        WHATSAPP_STICKER.max_size.bytes(),
        OUTPUT_S,
        &conversion.report,
        &conversion.path,
    );
    eprintln!(
        "fluid {content:?}: {} bytes, [{}], {} complete encodings, {seconds:.0} s",
        conversion.report.size_bytes(),
        conversion.report.attempt,
        conversion.report.complete_encodings,
    );
    (conversion.report, seconds)
}

#[test]
#[ignore = "slow benchmark"]
fn fluid_keeps_24_fps_on_a_grainy_scene_with_more_detail_than_before() {
    let (report, _) = fluid(Content::GrainyScene);
    assert_eq!(report.attempt.fps.value(), 24);
    // Before denoising this clip fitted at quality 5 (456 KB); now it keeps full detail at 14 with keyframes every 10 frames.
    assert_eq!(report.attempt.detail, Ratio::WHOLE);
    assert!(report.attempt.value >= 12, "{}", report.attempt);
}

#[test]
#[ignore = "slow benchmark"]
fn fluid_keeps_24_fps_on_a_fractal_zoom() {
    let (report, _) = fluid(Content::Mandelbrot);
    assert_eq!(report.attempt.fps.value(), 24);
    // Nothing to denoise here: it must stay as good as before (87.5 % content, detail 50 %, quality 0).
    assert!(report.attempt.value == 0 && report.attempt.detail_side() == 256);
}
