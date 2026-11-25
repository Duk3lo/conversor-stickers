#![allow(clippy::panic, clippy::expect_used)]

mod common;

use conversor_core::{
    ConversionError, ConversionOptions, ConversionParams, OptionsError, PRESETS, Preset, Speed,
    WHATSAPP_STICKER,
};

use common::{TestVideo, check_rules, convert_video, convert_video_with};

fn pattern(duration_s: f64) -> TestVideo {
    TestVideo::new(common::Content::Pattern, 320, 180, duration_s)
}

fn params(
    preset: &Preset,
    start_s: Option<f64>,
    max_seconds: Option<f64>,
    automatic_fitting: bool,
) -> ConversionParams {
    ConversionParams {
        start_s,
        max_seconds,
        automatic_fitting,
        fps: Some(10),
        ..ConversionParams::for_preset(*preset)
    }
}

fn convert_y_check(
    preset: &Preset,
    video: &TestVideo,
    params: &ConversionParams,
    expected_s: f64,
    factor: f64,
) {
    let conversion = convert_video(video, params)
        .unwrap_or_else(|e| panic!("{preset}, video de {} s, {params:?}: {e}", video.duration_s));
    check_rules(
        preset,
        preset.max_size.bytes(),
        expected_s,
        &conversion.report,
        &conversion.path,
    );
    assert!(
        (conversion.report.plan.factor_speed - factor).abs() < 1e-9,
        "{preset}: factor {} y se esperaba {factor}",
        conversion.report.plan.factor_speed
    );
}

fn wait_not_enters_nor_accelerated(
    preset: &Preset,
    video: &TestVideo,
    params: &ConversionParams,
) -> (f64, f64) {
    match convert_video(video, params) {
        Err(ConversionError::Options(OptionsError::DoesNotFitEvenSpedUp {
            crop_s,
            suggested_start_s,
            ..
        })) => (crop_s, suggested_start_s),
        Err(other) => panic!("{preset}: se esperaba NoEntraNiAcelerado y salió {other:?}"),
        Ok(_) => panic!("{preset}: se esperaba NoEntraNiAcelerado y convirtió"),
    }
}

fn fit_with_video_of(duration_s: f64) {
    let video = pattern(duration_s);
    for preset in PRESETS {
        let cap_s = preset.max_duration_s;
        let params = params(preset, None, None, true);
        if duration_s <= cap_s {
            convert_y_check(preset, &video, &params, duration_s, 1.0);
        } else if duration_s <= 2.0 * cap_s {
            convert_y_check(preset, &video, &params, cap_s, duration_s / cap_s);
        } else {
            wait_not_enters_nor_accelerated(preset, &video, &params);
        }
    }
}

#[test]
fn fit_with_8_s() {
    fit_with_video_of(8.0);
}

#[test]
fn fit_with_12_s() {
    fit_with_video_of(12.0);
}

#[test]
fn fit_with_19_5_s() {
    fit_with_video_of(19.5);
}

#[test]
fn fit_with_20_s() {
    fit_with_video_of(20.0);
}

#[test]
fn fit_with_25_s() {
    fit_with_video_of(25.0);
}

#[test]
fn fit_with_over_double_of_cap_returns_crop_and_follow_it_works() {
    for preset in PRESETS {
        let video = TestVideo::new(
            common::Content::Pattern,
            160,
            90,
            2.0 * preset.max_duration_s + 5.0,
        );
        let without_crop = ConversionParams {
            fps: Some(5),
            ..params(preset, None, None, true)
        };
        let (crop_s, suggested_start_s) =
            wait_not_enters_nor_accelerated(preset, &video, &without_crop);
        assert_eq!(crop_s, 5.0, "{preset}");
        assert_eq!(suggested_start_s, 5.0, "{preset}");

        let cropped = ConversionParams {
            start_s: Some(suggested_start_s),
            ..without_crop
        };
        convert_y_check(preset, &video, &cropped, preset.max_duration_s, 2.0);
    }
}

struct Case {
    duration_s: f64,
    start_s: Option<f64>,
    max_seconds: Option<f64>,
    automatic_fitting: bool,
    output: Result<(f64, f64), f64>,
}

#[test]
fn start_and_max_seconds_give_range_and_speed_expected() {
    let cases = [
        Case {
            duration_s: 8.0,
            start_s: None,
            max_seconds: Some(4.0),
            automatic_fitting: false,
            output: Ok((4.0, 1.0)),
        },
        Case {
            duration_s: 8.0,
            start_s: Some(6.0),
            max_seconds: None,
            automatic_fitting: false,
            output: Ok((2.0, 1.0)),
        },
        Case {
            duration_s: 8.0,
            start_s: None,
            max_seconds: Some(4.0),
            automatic_fitting: true,
            output: Ok((4.0, 2.0)),
        },
        Case {
            duration_s: 9.0,
            start_s: Some(2.0),
            max_seconds: Some(4.0),
            automatic_fitting: true,
            output: Ok((4.0, 1.75)),
        },
        Case {
            duration_s: 6.0,
            start_s: Some(3.0),
            max_seconds: Some(4.0),
            automatic_fitting: true,
            output: Ok((3.0, 1.0)),
        },
        Case {
            duration_s: 12.0,
            start_s: None,
            max_seconds: Some(4.0),
            automatic_fitting: true,
            output: Err(4.0),
        },
        Case {
            duration_s: 12.0,
            start_s: Some(1.0),
            max_seconds: Some(4.0),
            automatic_fitting: true,
            output: Err(3.0),
        },
    ];
    for preset in PRESETS {
        for case in &cases {
            let video = pattern(case.duration_s);
            let params = params(
                preset,
                case.start_s,
                case.max_seconds,
                case.automatic_fitting,
            );
            match case.output {
                Ok((expected_s, factor)) => {
                    convert_y_check(preset, &video, &params, expected_s, factor);
                }
                Err(crop_expected_s) => {
                    let (crop_s, suggested_start_s) =
                        wait_not_enters_nor_accelerated(preset, &video, &params);
                    assert_eq!(crop_s, crop_expected_s, "{preset}");
                    let expected_start_s = case.start_s.unwrap_or(0.0) + crop_expected_s;
                    assert_eq!(suggested_start_s, expected_start_s, "{preset}");
                    let cropped = ConversionParams {
                        start_s: Some(suggested_start_s),
                        ..params
                    };
                    convert_y_check(preset, &video, &cropped, 4.0, 2.0);
                }
            }
        }
    }
}

#[test]
fn start_outside_of_video_or_max_seconds_over_cap_give_error_typed() {
    let video = pattern(8.0);
    for preset in PRESETS {
        let start = params(preset, Some(8.0), None, true);
        assert!(
            matches!(
                convert_video(&video, &start),
                Err(ConversionError::Options(
                    OptionsError::StartOutsideOfVideo { .. }
                ))
            ),
            "{preset}: --start igual a la duración"
        );
        let long = params(preset, None, Some(preset.max_duration_s + 0.5), true);
        assert!(
            matches!(
                convert_video(&video, &long),
                Err(ConversionError::Options(
                    OptionsError::MaxSecondsOutsideOfRange { .. }
                ))
            ),
            "{preset}: --max-seconds sobre el tope"
        );
    }
}

fn follow_suggestion_of_seconds_fits(video: &TestVideo, fps: u32) {
    let with_fit = ConversionParams {
        fps: Some(fps),
        ..params(&WHATSAPP_STICKER, None, None, true)
    };
    let first = convert_video(video, &with_fit);
    let Err(error @ ConversionError::DoesNotFitInSize { .. }) = first else {
        panic!("se esperaba NoCabeEnPeso con {}", video.file_name());
    };
    let message = error.to_string();
    let ConversionError::DoesNotFitInSize {
        suggested_seconds: Some(suggested_seconds),
        ..
    } = error
    else {
        panic!("no sugirió ninguna duración: {message}");
    };
    assert!(message.contains("sin --fit"), "{message}");

    let following_suggestion = ConversionParams {
        max_seconds: Some(suggested_seconds),
        automatic_fitting: false,
        ..with_fit
    };
    match convert_video(video, &following_suggestion) {
        Ok(conversion) => check_rules(
            &WHATSAPP_STICKER,
            WHATSAPP_STICKER.max_size.bytes(),
            suggested_seconds,
            &conversion.report,
            &conversion.path,
        ),
        Err(error) => panic!("con --max-seconds {suggested_seconds} sigue sin caber: {error}"),
    };
}

#[test]
fn suggestion_of_seconds_fits() {
    follow_suggestion_of_seconds_fits(&TestVideo::mandelbrot(320, 180, 10.0), 10);
}

#[test]
fn suggestion_of_seconds_with_fit_accelerated_fits() {
    follow_suggestion_of_seconds_fits(&TestVideo::mandelbrot(320, 180, 14.0), 15);
}

fn a_speed(speed: f64, max_seconds: f64, fps: u32) -> ConversionOptions {
    ConversionOptions::new(&ConversionParams {
        fps: Some(fps),
        ..params(&WHATSAPP_STICKER, None, Some(max_seconds), false)
    })
    .expect("opciones válidas")
    .with_speed(Speed::new(speed).expect("velocidad válida"))
    .expect("sin --fit se puede fijar la velocidad")
}

#[test]
fn speed_fixed_changes_duration_of_output() {
    for (video_s, speed, output_s) in [(3.0, 0.5, 6.0), (9.0, 3.0, 3.0)] {
        for fps in [12, 15, 20] {
            let video = pattern(video_s);
            let options = a_speed(speed, 10.0, fps);
            let conversion = convert_video_with(&video, &options)
                .unwrap_or_else(|error| panic!("{speed}x a {fps} fps: {error}"));
            let animation = check_rules(
                &WHATSAPP_STICKER,
                WHATSAPP_STICKER.max_size.bytes(),
                output_s,
                &conversion.report,
                &conversion.path,
            );
            let total_ms = animation.total_duration_ms() as f64;
            assert!(
                (total_ms - output_s * 1000.0).abs() <= 10.0,
                "{speed}x a {fps} fps: {total_ms} ms"
            );
            assert_eq!(conversion.report.plan.factor_speed, speed);
            assert_eq!(conversion.report.plan.input_duration_s, video_s);
        }
    }
}

#[test]
fn suggestion_of_seconds_with_speed_fixed_fits() {
    let video = TestVideo::mandelbrot(320, 180, 14.0);
    let Err(error @ ConversionError::DoesNotFitInSize { .. }) =
        convert_video_with(&video, &a_speed(1.5, 10.0, 15))
    else {
        panic!("se esperaba NoCabeEnPeso con {}", video.file_name());
    };
    let message = error.to_string();
    let ConversionError::DoesNotFitInSize {
        suggested_seconds: Some(suggested_seconds),
        remove_fit: false,
        ..
    } = error
    else {
        panic!("no sugirió ninguna duración: {message}");
    };
    assert!(message.contains("a 1,5x"), "{message}");

    let conversion = convert_video_with(&video, &a_speed(1.5, suggested_seconds, 15))
        .unwrap_or_else(|error| panic!("con {suggested_seconds} s sigue sin caber: {error}"));
    check_rules(
        &WHATSAPP_STICKER,
        WHATSAPP_STICKER.max_size.bytes(),
        suggested_seconds,
        &conversion.report,
        &conversion.path,
    );
}
