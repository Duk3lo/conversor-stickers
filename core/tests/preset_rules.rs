#![allow(clippy::panic, clippy::expect_used)]

mod common;

use std::fs;

use conversor_core::{ConversionError, ConversionParams, Format, OptionsError, PRESETS, Preset};

use common::{
    TestVideo, check_duration_except_rules, check_requested_duration, check_rules, convert_video,
};

fn short_noise() -> TestVideo {
    TestVideo::new(common::Content::Noise, 320, 180, 1.0)
}

fn noise_that_does_not_fit(preset: &Preset) -> TestVideo {
    match preset.format {
        Format::AnimatedWebp => TestVideo::new(common::Content::RandomNoise, 640, 360, 3.0),
        Format::Gif => TestVideo::new(common::Content::RandomNoise, 128, 128, 8.0),
        Format::Apng => TestVideo::new(common::Content::RandomNoise, 128, 128, 5.0),
    }
}

fn min_size_params(preset: &Preset) -> (ConversionParams, u64) {
    let params = ConversionParams {
        max_mb: preset.range_max_mb.map(|range| range.min),
        ..ConversionParams::for_preset(*preset)
    };
    let size_bytes = match preset.range_max_mb {
        Some(range) => (range.min * 1_000_000.0).round() as u64,
        None => preset.max_size.bytes(),
    };
    (params, size_bytes)
}

fn all_presets_meet_rules(video: &TestVideo) {
    for preset in PRESETS {
        let conversion = convert_video(video, &ConversionParams::for_preset(*preset))
            .unwrap_or_else(|e| panic!("{preset} sobre {}: {e}", video.file_name()));
        check_rules(
            preset,
            preset.max_size.bytes(),
            video.duration_s,
            &conversion.report,
            &conversion.path,
        );
    }
}

#[test]
fn all_presets_meet_rules_with_video_horizontal() {
    all_presets_meet_rules(&TestVideo::horizontal(3.0));
}

#[test]
fn all_presets_meet_rules_with_video_vertical() {
    all_presets_meet_rules(&TestVideo::vertical(3.0));
}

fn static_video() -> TestVideo {
    TestVideo::new(common::Content::Static, 640, 360, 3.0)
}

#[test]
fn all_presets_meet_rules_with_video_static_except_duration() {
    let video = static_video();
    for preset in PRESETS {
        let conversion = convert_video(&video, &ConversionParams::for_preset(*preset))
            .unwrap_or_else(|e| panic!("{preset} sobre estático: {e}"));
        check_duration_except_rules(
            preset,
            preset.max_size.bytes(),
            &conversion.report,
            &conversion.path,
        );
    }
}

#[test]
fn video_static_keeps_duration_requested_in_each_preset() {
    let video = static_video();
    for preset in PRESETS {
        let conversion = convert_video(&video, &ConversionParams::for_preset(*preset))
            .unwrap_or_else(|e| panic!("{preset} sobre estático: {e}"));
        let animation = check_duration_except_rules(
            preset,
            preset.max_size.bytes(),
            &conversion.report,
            &conversion.path,
        );
        check_requested_duration(preset, video.duration_s, &animation);
    }
}

#[test]
fn all_presets_meet_rules_with_video_anamorphic() {
    all_presets_meet_rules(&TestVideo::horizontal(3.0).with_sar(4, 3));
}

#[test]
fn noise_force_ladder_and_file_follows_meeting_rules() {
    let video = short_noise();
    for preset in PRESETS {
        let (params, size_bytes) = min_size_params(preset);
        let conversion =
            convert_video(&video, &params).unwrap_or_else(|e| panic!("{preset} sobre ruido: {e}"));
        assert!(
            !conversion.report.reductions.is_empty(),
            "{preset}: el ruido no hizo falta reducir nada ({} B de {size_bytes} B)",
            conversion.report.size_bytes()
        );
        check_rules(
            preset,
            size_bytes,
            video.duration_s,
            &conversion.report,
            &conversion.path,
        );
    }
}

#[test]
fn rejects_max_mb_invalid_or_not_applicable_with_error_typed() {
    let video = TestVideo::horizontal(3.0);
    for preset in PRESETS {
        let cases: Vec<f64> = match preset.range_max_mb {
            None => vec![1.0],
            Some(range) => vec![range.min / 2.0, range.max + 1.0, 0.0, -1.0, f64::NAN],
        };
        for mb in cases {
            let params = ConversionParams {
                max_mb: Some(mb),
                ..ConversionParams::for_preset(*preset)
            };
            let error = match convert_video(&video, &params) {
                Err(error) => error,
                Ok(_) => panic!("{preset}: --max-mb {mb} debía rechazarse"),
            };
            match (preset.range_max_mb, error) {
                (
                    None,
                    ConversionError::Options(OptionsError::MaxMbNotSupported { preset: name }),
                ) => assert_eq!(name, preset.name),
                (
                    Some(range),
                    ConversionError::Options(OptionsError::MaxMbOutsideOfRange {
                        preset: name,
                        min,
                        max,
                        ..
                    }),
                ) => {
                    assert_eq!(name, preset.name);
                    assert_eq!((min, max), (range.min, range.max));
                }
                (_, other) => panic!("{preset}: --max-mb {mb} dio {other:?}"),
            }
        }
    }
}

#[test]
fn with_size_min_noise_returns_not_fits_in_size_with_message_util() {
    for preset in PRESETS {
        let video = noise_that_does_not_fit(preset);
        let (params, size_bytes) = min_size_params(preset);
        let params = ConversionParams {
            max_seconds: Some(video.duration_s),
            ..params
        };
        let error = match convert_video(&video, &params) {
            Err(error) => error,
            Ok(conversion) => panic!(
                "{preset}: el ruido cupo en {size_bytes} B con {} B",
                conversion.report.size_bytes()
            ),
        };
        let message = error.to_string();
        let ConversionError::DoesNotFitInSize {
            preset: name,
            limit_bytes,
            best_bytes,
            suggested_seconds,
            ..
        } = error
        else {
            panic!("{preset}: se esperaba NoCabeEnPeso y salió {error:?}");
        };
        assert_eq!(name, preset.name);
        assert_eq!(limit_bytes, size_bytes);
        assert!(
            best_bytes > limit_bytes,
            "{preset}: dice que no cabe pero el mejor intento pesa {best_bytes} B <= {limit_bytes} B"
        );
        match suggested_seconds {
            Some(seconds) => {
                assert!(
                    (0.5..video.duration_s).contains(&seconds),
                    "{preset}: sugiere {seconds} s para un video de {} s",
                    video.duration_s
                );
                assert!(message.contains("--max-seconds"), "{message}");
            }
            None => assert!(!message.contains("--max-seconds"), "{message}"),
        }
        assert!(message.starts_with("no cabe en "), "{message}");
        assert!(message.contains("--start"), "{message}");
    }
}

#[test]
fn same_video_and_options_give_same_bytes_without_and_with_ladder() {
    let pattern = TestVideo::horizontal(3.0);
    let noise = short_noise();
    for preset in PRESETS {
        let (with_ladder, _) = min_size_params(preset);
        for (video, params) in [
            (&pattern, ConversionParams::for_preset(*preset)),
            (&noise, with_ladder),
        ] {
            let first = convert_video(video, &params).expect("convierte");
            let second = convert_video(video, &params).expect("convierte");
            let bytes = |path| fs::read(path).expect("la salida existe");
            assert!(
                bytes(&first.path) == bytes(&second.path),
                "{preset} sobre {}: los bytes difieren",
                video.file_name()
            );
            assert_eq!(first.report.reductions, second.report.reductions);
        }
    }
}

#[test]
fn not_fits_in_size_only_if_best_attempt_exceeds_limit() {
    let video = TestVideo::mandelbrot(160, 90, 8.0);
    let params = ConversionParams {
        max_seconds: Some(7.0),
        ..ConversionParams::for_preset(conversor_core::WHATSAPP_STICKER)
    };
    match convert_video(&video, &params) {
        Ok(_) => {}
        Err(ConversionError::DoesNotFitInSize {
            limit_bytes,
            best_bytes,
            ..
        }) => assert!(
            best_bytes > limit_bytes,
            "dice que no cabe, pero el intento más ligero pesó {best_bytes} B y el límite es {limit_bytes} B"
        ),
        Err(other) => panic!("error inesperado: {other}"),
    }
}
