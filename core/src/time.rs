use crate::error::OptionsError;
use crate::options::{ConversionOptions, Fps, Speed};

const FACTOR_MAX: f64 = 2.0;

const TOLERANCE_S: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimePlan {
    pub start_s: f64,
    pub input_duration_s: f64,
    pub factor_speed: f64,
    pub output_duration_s: f64,
}

impl TimePlan {
    pub fn frame_count(&self, fps: Fps) -> usize {
        (self.output_duration_s * f64::from(fps.value())).floor() as usize
    }
}

pub fn plan_time(
    video_duration_s: f64,
    options: &ConversionOptions,
) -> Result<TimePlan, OptionsError> {
    let start_s = options.start_s();
    if !video_duration_s.is_finite() || start_s >= video_duration_s {
        return Err(OptionsError::StartOutsideOfVideo {
            start_s,
            video_duration_s,
        });
    }

    let available_s = video_duration_s - start_s;
    let target_s = options.target_duration_s();

    if let Some(speed) = options.speed() {
        return Ok(plan_a_speed(start_s, available_s, target_s, speed));
    }
    if !options.automatic_fitting() || available_s <= target_s {
        let duration_s = available_s.min(target_s);
        return Ok(TimePlan {
            start_s,
            input_duration_s: duration_s,
            factor_speed: 1.0,
            output_duration_s: duration_s,
        });
    }

    let max_s = FACTOR_MAX * target_s;
    if available_s > max_s {
        let crop_s = seconds_a_crop(available_s - max_s);
        return Err(OptionsError::DoesNotFitEvenSpedUp {
            available_s,
            max_s,
            crop_s,
            suggested_start_s: tenths_towards_above(start_s + crop_s),
        });
    }

    Ok(TimePlan {
        start_s,
        input_duration_s: available_s,
        factor_speed: available_s / target_s,
        output_duration_s: target_s,
    })
}

fn plan_a_speed(start_s: f64, available_s: f64, target_s: f64, speed: Speed) -> TimePlan {
    let speed = speed.value();
    let max_range_s = target_s * speed;
    let arrives_al_target = available_s + TOLERANCE_S >= max_range_s;
    TimePlan {
        start_s,
        input_duration_s: max_range_s.min(available_s),
        factor_speed: speed,
        output_duration_s: if arrives_al_target {
            target_s
        } else {
            available_s / speed
        },
    }
}

pub(crate) fn frame_duration_ms(index: u64, fps: Fps) -> u32 {
    let duration = frame_mark_ms(index.saturating_add(1), fps) - frame_mark_ms(index, fps);
    u32::try_from(duration).unwrap_or(u32::MAX)
}

pub(crate) fn frame_mark_ms(index: u64, fps: Fps) -> u64 {
    let fps = u64::from(fps.value());
    (index.saturating_mul(1000) + fps / 2) / fps
}

fn seconds_a_crop(excess_s: f64) -> f64 {
    tenths_towards_above(excess_s).max(0.1)
}

fn tenths_towards_above(seconds: f64) -> f64 {
    (seconds * 10.0 - 1e-9).ceil() / 10.0
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::options::{ConversionParams, Speed};
    use crate::preset::{PRESETS, Preset, WHATSAPP_STICKER};

    fn options(
        preset: Preset,
        max_seconds: Option<f64>,
        start_s: f64,
        automatic_fitting: bool,
    ) -> ConversionOptions {
        ConversionOptions::new(&ConversionParams {
            max_seconds,
            start_s: Some(start_s),
            automatic_fitting,
            ..ConversionParams::for_preset(preset)
        })
        .unwrap()
    }

    fn whatsapp_fit() -> ConversionOptions {
        options(WHATSAPP_STICKER, None, 0.0, true)
    }

    #[test]
    fn without_fit_video_long_short_at_the_target() {
        let plan = plan_time(25.0, &options(WHATSAPP_STICKER, None, 0.0, false)).unwrap();
        assert_eq!(
            plan,
            TimePlan {
                start_s: 0.0,
                input_duration_s: 6.0,
                factor_speed: 1.0,
                output_duration_s: 6.0,
            }
        );
    }

    #[test]
    fn without_fit_video_short_uses_whole_from_start() {
        let plan = plan_time(5.0, &options(WHATSAPP_STICKER, Some(10.0), 2.0, false)).unwrap();
        assert_eq!(plan.start_s, 2.0);
        assert_eq!(plan.input_duration_s, 3.0);
        assert_eq!(plan.output_duration_s, 3.0);
        assert_eq!(plan.factor_speed, 1.0);
    }

    #[test]
    fn with_fit_10_s_enter_without_accelerate() {
        let plan = plan_time(10.0, &whatsapp_fit()).unwrap();
        assert_eq!(plan.factor_speed, 1.0);
        assert_eq!(plan.input_duration_s, 10.0);
        assert_eq!(plan.output_duration_s, 10.0);
    }

    #[test]
    fn with_fit_12_s_accelerate_1_2x() {
        let plan = plan_time(12.0, &whatsapp_fit()).unwrap();
        assert!((plan.factor_speed - 1.2).abs() < 1e-9);
        assert_eq!(plan.input_duration_s, 12.0);
        assert_eq!(plan.output_duration_s, 10.0);
    }

    #[test]
    fn with_fit_20_s_accelerate_2x() {
        let plan = plan_time(20.0, &whatsapp_fit()).unwrap();
        assert_eq!(plan.factor_speed, 2.0);
        assert_eq!(plan.output_duration_s, 10.0);
    }

    #[test]
    fn with_fit_25_s_not_enter_and_has_that_crop_5_s() {
        let error = plan_time(25.0, &whatsapp_fit()).unwrap_err();
        assert_eq!(
            error,
            OptionsError::DoesNotFitEvenSpedUp {
                available_s: 25.0,
                max_s: 20.0,
                crop_s: 5.0,
                suggested_start_s: 5.0,
            }
        );
        assert_eq!(
            error.to_string(),
            "el tramo dura 25,0 s; acelerando 2x caben 20,0 s en 10,0 s. \
             Recorta al menos 5,0 s (por ejemplo, --start 5)."
        );
    }

    #[test]
    fn crop_rounds_towards_above_to_tenths_and_sum_start() {
        let error = plan_time(23.04, &options(WHATSAPP_STICKER, None, 1.5, true)).unwrap_err();
        assert_eq!(
            error,
            OptionsError::DoesNotFitEvenSpedUp {
                available_s: 23.04 - 1.5,
                max_s: 20.0,
                crop_s: 1.6,
                suggested_start_s: 3.1,
            }
        );
        assert!(
            error.to_string().ends_with("(por ejemplo, --start 3,1)."),
            "{error}"
        );
    }

    #[test]
    fn with_fit_target_is_max_seconds_if_passes() {
        let plan = plan_time(8.0, &options(WHATSAPP_STICKER, Some(4.0), 0.0, true)).unwrap();
        assert_eq!(plan.factor_speed, 2.0);
        assert_eq!(plan.output_duration_s, 4.0);
    }

    #[test]
    fn rejects_duration_to_later_or_equal_start() {
        for (duration, start) in [(10.0, 10.0), (10.0, 12.0), (0.0, 0.0), (f64::NAN, 0.0)] {
            for fit in [false, true] {
                let error =
                    plan_time(duration, &options(WHATSAPP_STICKER, None, start, fit)).unwrap_err();
                assert!(
                    matches!(error, OptionsError::StartOutsideOfVideo { .. }),
                    "duración {duration}, inicio {start}: {error:?}"
                );
            }
        }
    }

    #[test]
    fn number_of_frames_is_floor_of_duration_by_fps() {
        let plan = plan_time(12.0, &whatsapp_fit()).unwrap();
        assert_eq!(plan.frame_count(Fps::new(15).unwrap()), 150);

        let plan = plan_time(3.05, &options(WHATSAPP_STICKER, None, 0.0, false)).unwrap();
        assert_eq!(plan.frame_count(Fps::new(15).unwrap()), 45);
        assert_eq!(plan.frame_count(Fps::new(30).unwrap()), 91);
    }

    #[test]
    fn marks_round_at_the_millisecond_without_derive() {
        let fps = Fps::new(15).unwrap();
        let marks: Vec<u64> = (0..=4).map(|i| frame_mark_ms(i, fps)).collect();
        assert_eq!(marks, [0, 67, 133, 200, 267]);
        let durations: Vec<u32> = (0..4).map(|i| frame_duration_ms(i, fps)).collect();
        assert_eq!(durations, [67, 66, 67, 67]);
        let total: u64 = (0..150).map(|i| u64::from(frame_duration_ms(i, fps))).sum();
        assert_eq!(total, 10_000);
    }

    #[test]
    fn with_speed_range_is_target_by_speed() {
        let cases = [
            (3.0, 0.0, 10.0, 0.5, 3.0, 6.0),
            (25.0, 0.0, 10.0, 0.5, 5.0, 10.0),
            (9.0, 0.0, 10.0, 3.0, 9.0, 3.0),
            (40.0, 2.0, 10.0, 3.0, 30.0, 10.0),
            (25.0, 0.0, 6.0, 1.5, 9.0, 6.0),
            (25.0, 20.0, 6.0, 1.0, 5.0, 5.0),
            (11.0, 0.0, 10.0, 1.1, 11.0, 10.0),
        ];
        for (video_s, start_s, target_s, speed, range_s, output_s) in cases {
            let options = options(WHATSAPP_STICKER, Some(target_s), start_s, false)
                .with_speed(Speed::new(speed).unwrap())
                .unwrap();
            let plan = plan_time(video_s, &options).unwrap();
            let case = format!("{video_s} s desde {start_s} a {speed}x: {plan:?}");
            assert_eq!(plan.start_s, start_s, "{case}");
            assert_eq!(plan.factor_speed, speed, "{case}");
            assert!((plan.input_duration_s - range_s).abs() < 1e-9, "{case}");
            assert!((plan.output_duration_s - output_s).abs() < 1e-9, "{case}");
            let fps = Fps::DEFAULT;
            let frames = (output_s * f64::from(fps.value())) as usize;
            assert_eq!(plan.frame_count(fps), frames, "{case}");
        }
    }

    #[test]
    fn speed_normal_plans_as_without_speed() {
        let without_speed = options(WHATSAPP_STICKER, None, 1.0, false);
        let normal = without_speed.with_speed(Speed::NORMAL).unwrap();
        for video_s in [3.05, 7.0, 25.0] {
            assert_eq!(
                plan_time(video_s, &normal),
                plan_time(video_s, &without_speed)
            );
        }
    }

    fn preset() -> impl Strategy<Value = Preset> {
        proptest::sample::select(PRESETS.to_vec())
    }

    proptest! {
        #[test]
        fn plan_never_exits_of_rules(
            preset in preset(),
            video_duration_s in 0.1f64..120.0,
            start_s in 0.0f64..130.0,
            fraction_max_seconds in proptest::option::of(0.01f64..=1.0),
            fps in Fps::MIN..=Fps::MAX,
            automatic_fitting in any::<bool>(),
        ) {
            let cap_s = preset.max_duration_s;
            let max_seconds = fraction_max_seconds.map(|f| f * cap_s);
            let options = ConversionOptions::new(&ConversionParams {
                max_seconds,
                start_s: Some(start_s),
                fps: Some(fps),
                automatic_fitting,
                ..ConversionParams::for_preset(preset)
            })
            .unwrap();
            let fps = options.fps();
            let target_s = options.target_duration_s();
            let available_s = video_duration_s - start_s;

            match plan_time(video_duration_s, &options) {
                Ok(plan) => {
                    prop_assert!(start_s < video_duration_s);
                    prop_assert!(!(automatic_fitting && available_s > 2.0 * target_s));
                    prop_assert!(plan.output_duration_s <= cap_s);
                    prop_assert!(plan.output_duration_s <= target_s);
                    prop_assert!((1.0..=2.0).contains(&plan.factor_speed));
                    prop_assert!(automatic_fitting || plan.factor_speed == 1.0);
                    prop_assert!(plan.start_s + plan.input_duration_s <= video_duration_s + 1e-9);
                    let frames = plan.frame_count(fps) as f64;
                    prop_assert!(
                        frames * 1000.0 / f64::from(fps.value()) <= target_s * 1000.0
                    );
                    let total_ms: u64 = (0..frames as u64)
                        .map(|i| u64::from(frame_duration_ms(i, fps)))
                        .sum();
                    prop_assert!(total_ms as f64 <= cap_s * 1000.0);
                }
                Err(OptionsError::StartOutsideOfVideo { .. }) => {
                    prop_assert!(start_s >= video_duration_s);
                }
                Err(OptionsError::DoesNotFitEvenSpedUp { max_s, crop_s, suggested_start_s, .. }) => {
                    prop_assert!(automatic_fitting);
                    prop_assert!(start_s < video_duration_s);
                    prop_assert!(available_s > 2.0 * target_s);
                    prop_assert_eq!(max_s, 2.0 * target_s);
                    prop_assert!(available_s - crop_s <= max_s + 1e-9);
                    prop_assert!(crop_s - (available_s - max_s) < 0.1 + 1e-9);
                    prop_assert!(video_duration_s - suggested_start_s <= max_s + 1e-9);
                    prop_assert!(suggested_start_s - (start_s + crop_s) < 0.1 + 1e-9);
                }
                Err(other) => prop_assert!(false, "error inesperado: {other:?}"),
            }
        }

        #[test]
        fn with_speed_plan_never_exits_of_rules(
            preset in preset(),
            video_duration_s in 0.1f64..120.0,
            start_s in 0.0f64..130.0,
            fraction_max_seconds in 0.01f64..=1.0,
            speed in Speed::MIN..=Speed::MAX,
        ) {
            let target_s = fraction_max_seconds * preset.max_duration_s;
            let options = options(preset, Some(target_s), start_s, false)
                .with_speed(Speed::new(speed).unwrap())
                .unwrap();
            match plan_time(video_duration_s, &options) {
                Ok(plan) => {
                    let available_s = video_duration_s - start_s;
                    prop_assert_eq!(plan.factor_speed, speed);
                    prop_assert!((0.5..=3.0).contains(&plan.factor_speed));
                    prop_assert!(plan.output_duration_s <= target_s);
                    prop_assert!(plan.input_duration_s <= available_s);
                    prop_assert!(
                        (plan.output_duration_s * speed - plan.input_duration_s).abs() < 1e-8
                    );
                }
                Err(OptionsError::StartOutsideOfVideo { .. }) => {
                    prop_assert!(start_s >= video_duration_s);
                }
                Err(other) => prop_assert!(false, "error inesperado: {other:?}"),
            }
        }
    }
}
