use crate::error::{OptionsError, SpeedError};
use crate::preset::Preset;

const DEFAULT_DURATION_S: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fps(u32);

impl Fps {
    pub const MIN: u32 = 5;
    pub const MAX: u32 = 30;
    pub const DEFAULT: Fps = Fps(15);

    pub fn new(value: u32) -> Result<Self, OptionsError> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Fps(value))
        } else {
            Err(OptionsError::FpsOutsideOfRange { value })
        }
    }

    pub(crate) const fn constant<const VALUE: u32>() -> Fps {
        const { assert!(VALUE >= Fps::MIN && VALUE <= Fps::MAX) };
        Fps(VALUE)
    }

    pub fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Speed(f64);

impl Speed {
    pub const MIN: f64 = 0.5;
    pub const MAX: f64 = 3.0;
    pub const NORMAL: Speed = Speed(1.0);

    pub fn new(value: f64) -> Result<Self, SpeedError> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Speed(value))
        } else {
            Err(SpeedError::OutsideRange { value })
        }
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Priority {
    Sharpness,
    #[default]
    Balance,
    Smoothness,
    FitAnyway,
    Fluid,
}

impl Priority {
    pub fn default_fps(self) -> Fps {
        match self {
            Priority::Sharpness => Fps::constant::<12>(),
            Priority::Balance | Priority::FitAnyway => Fps::DEFAULT,
            Priority::Smoothness | Priority::Fluid => Fps::constant::<20>(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaxSize(u64);

impl MaxSize {
    pub const fn from_bytes(bytes: u64) -> Self {
        MaxSize(bytes)
    }

    pub fn bytes(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConversionParams {
    pub preset: Preset,
    pub max_seconds: Option<f64>,
    pub start_s: Option<f64>,
    pub fps: Option<u32>,
    pub automatic_fitting: bool,
    pub max_mb: Option<f64>,
    pub preview: bool,
}

impl ConversionParams {
    pub fn for_preset(preset: Preset) -> Self {
        ConversionParams {
            preset,
            max_seconds: None,
            start_s: None,
            fps: None,
            automatic_fitting: false,
            max_mb: None,
            preview: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConversionOptions {
    preset: Preset,
    target_duration_s: f64,
    start_s: f64,
    fps: Fps,
    automatic_fitting: bool,
    max_size: MaxSize,
    preview: bool,
    speed: Option<Speed>,
    priority: Priority,
}

impl ConversionOptions {
    pub fn new(params: &ConversionParams) -> Result<Self, OptionsError> {
        let preset = params.preset;
        let cap_s = preset.max_duration_s;

        let target_duration_s = match params.max_seconds {
            Some(value) if !(value > 0.0 && value <= cap_s) => {
                return Err(OptionsError::MaxSecondsOutsideOfRange {
                    preset: preset.name,
                    value,
                    cap_s,
                });
            }
            Some(value) => value,
            None if params.automatic_fitting => cap_s,
            None => DEFAULT_DURATION_S.min(cap_s),
        };

        let start_s = params.start_s.unwrap_or(0.0);
        if !(start_s.is_finite() && start_s >= 0.0) {
            return Err(OptionsError::StartNotValid { value: start_s });
        }

        let fps = match params.fps {
            Some(value) => {
                let fps = Fps::new(value)?;
                if value > preset.max_fps() {
                    return Err(OptionsError::FpsNotSupported {
                        preset: preset.name,
                        value,
                    });
                }
                fps
            }
            None => Fps::DEFAULT,
        };

        let max_size = match (params.max_mb, preset.range_max_mb) {
            (None, _) => preset.max_size,
            (Some(_), None) => {
                return Err(OptionsError::MaxMbNotSupported {
                    preset: preset.name,
                });
            }
            (Some(mb), Some(range)) if !(range.min..=range.max).contains(&mb) => {
                return Err(OptionsError::MaxMbOutsideOfRange {
                    preset: preset.name,
                    value: mb,
                    min: range.min,
                    max: range.max,
                });
            }
            (Some(mb), Some(_)) => MaxSize::from_bytes((mb * 1_000_000.0).round() as u64),
        };

        Ok(ConversionOptions {
            preset,
            target_duration_s,
            start_s,
            fps,
            automatic_fitting: params.automatic_fitting,
            max_size,
            preview: params.preview,
            speed: None,
            priority: Priority::default(),
        })
    }

    pub fn with_speed(self, speed: Speed) -> Result<Self, SpeedError> {
        if self.automatic_fitting {
            return Err(SpeedError::WithAutomaticFitting);
        }
        Ok(ConversionOptions {
            speed: Some(speed),
            ..self
        })
    }

    #[must_use]
    pub fn with_priority(self, priority: Priority) -> Self {
        ConversionOptions { priority, ..self }
    }

    pub fn speed(&self) -> Option<Speed> {
        self.speed
    }

    pub fn priority(&self) -> Priority {
        self.priority
    }

    pub fn preset(&self) -> &Preset {
        &self.preset
    }

    pub fn target_duration_s(&self) -> f64 {
        self.target_duration_s
    }

    pub fn start_s(&self) -> f64 {
        self.start_s
    }

    pub fn fps(&self) -> Fps {
        self.fps
    }

    pub fn automatic_fitting(&self) -> bool {
        self.automatic_fitting
    }

    pub fn preview(&self) -> bool {
        self.preview
    }

    pub fn max_size(&self) -> MaxSize {
        self.max_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::{DISCORD_GIF, DISCORD_STICKER, WHATSAPP_STICKER};

    fn whatsapp() -> ConversionParams {
        ConversionParams::for_preset(WHATSAPP_STICKER)
    }

    fn discord_gif() -> ConversionParams {
        ConversionParams::for_preset(DISCORD_GIF)
    }

    #[test]
    fn default_values() {
        let options = ConversionOptions::new(&whatsapp()).unwrap();
        assert_eq!(options.target_duration_s(), 6.0);
        assert_eq!(options.start_s(), 0.0);
        assert_eq!(options.fps(), Fps::DEFAULT);
        assert_eq!(options.fps().value(), 15);
        assert!(!options.automatic_fitting());
        assert_eq!(options.max_size().bytes(), 500_000);
    }

    #[test]
    fn with_fit_and_without_max_seconds_target_is_cap_of_preset() {
        let options = ConversionOptions::new(&ConversionParams {
            automatic_fitting: true,
            ..whatsapp()
        })
        .unwrap();
        assert_eq!(options.target_duration_s(), 10.0);

        let options = ConversionOptions::new(&ConversionParams {
            automatic_fitting: true,
            ..discord_gif()
        })
        .unwrap();
        assert_eq!(options.target_duration_s(), 30.0);
    }

    #[test]
    fn if_cap_of_preset_is_smaller_that_value_default_sends_cap() {
        let params = ConversionParams::for_preset(DISCORD_STICKER);
        let options = ConversionOptions::new(&params).unwrap();
        assert_eq!(options.target_duration_s(), 5.0);

        let options = ConversionOptions::new(&ConversionParams {
            automatic_fitting: true,
            ..params
        })
        .unwrap();
        assert_eq!(options.target_duration_s(), 5.0);
    }

    #[test]
    fn max_seconds_inside_of_cap_respects() {
        for value in [0.5, 6.0, 10.0] {
            let options = ConversionOptions::new(&ConversionParams {
                max_seconds: Some(value),
                ..whatsapp()
            })
            .unwrap();
            assert_eq!(options.target_duration_s(), value);
        }
    }

    #[test]
    fn rejects_max_seconds_greater_that_cap_or_not_positive() {
        for value in [10.01, 11.0, 0.0, -1.0, f64::NAN, f64::INFINITY] {
            let error = ConversionOptions::new(&ConversionParams {
                max_seconds: Some(value),
                ..whatsapp()
            })
            .unwrap_err();
            assert!(
                matches!(
                    error,
                    OptionsError::MaxSecondsOutsideOfRange {
                        preset: "whatsapp-sticker",
                        cap_s: 10.0,
                        ..
                    }
                ),
                "valor {value}: {error:?}"
            );
        }
    }

    #[test]
    fn cap_of_max_seconds_depends_of_preset() {
        let params = ConversionParams {
            max_seconds: Some(30.0),
            ..discord_gif()
        };
        assert!(ConversionOptions::new(&params).is_ok());

        let params = ConversionParams {
            max_seconds: Some(30.5),
            ..discord_gif()
        };
        assert!(matches!(
            ConversionOptions::new(&params),
            Err(OptionsError::MaxSecondsOutsideOfRange { cap_s: 30.0, .. })
        ));
    }

    #[test]
    fn rejects_start_negative_or_not_finito() {
        for value in [-0.1, f64::NAN, f64::INFINITY] {
            let error = ConversionOptions::new(&ConversionParams {
                start_s: Some(value),
                ..whatsapp()
            })
            .unwrap_err();
            assert!(
                matches!(error, OptionsError::StartNotValid { .. }),
                "valor {value}: {error:?}"
            );
        }
    }

    #[test]
    fn accepts_fps_of_5_to_30() {
        for value in [5, 12, 30] {
            let options = ConversionOptions::new(&ConversionParams {
                fps: Some(value),
                ..whatsapp()
            })
            .unwrap();
            assert_eq!(options.fps().value(), value);
        }
    }

    #[test]
    fn rejects_fps_outside_of_5_to_30() {
        for value in [0, 4, 31, 60] {
            let error = ConversionOptions::new(&ConversionParams {
                fps: Some(value),
                ..whatsapp()
            })
            .unwrap_err();
            assert_eq!(error, OptionsError::FpsOutsideOfRange { value });
        }
    }

    #[test]
    fn max_mb_in_discord_gif_changes_size_max() {
        for (mb, bytes) in [(0.5, 500_000), (10.0, 10_000_000), (20.0, 20_000_000)] {
            let options = ConversionOptions::new(&ConversionParams {
                max_mb: Some(mb),
                ..discord_gif()
            })
            .unwrap();
            assert_eq!(options.max_size().bytes(), bytes);
        }
    }

    #[test]
    fn rejects_max_mb_outside_of_range() {
        for value in [0.49, 0.0, -1.0, 20.01, f64::NAN] {
            let error = ConversionOptions::new(&ConversionParams {
                max_mb: Some(value),
                ..discord_gif()
            })
            .unwrap_err();
            assert!(
                matches!(
                    error,
                    OptionsError::MaxMbOutsideOfRange {
                        preset: "discord-gif",
                        min: 0.5,
                        max: 20.0,
                        ..
                    }
                ),
                "valor {value}: {error:?}"
            );
        }
    }

    #[test]
    fn rejects_max_mb_in_preset_that_is_not_supported() {
        let error = ConversionOptions::new(&ConversionParams {
            max_mb: Some(5.0),
            ..whatsapp()
        })
        .unwrap_err();
        assert_eq!(
            error,
            OptionsError::MaxMbNotSupported {
                preset: "whatsapp-sticker"
            }
        );
    }

    #[test]
    fn speed_goes_of_0_5_to_3() {
        for value in [0.5, 1.0, 3.0] {
            assert_eq!(Speed::new(value).unwrap().value(), value);
        }
        assert_eq!(Speed::NORMAL, Speed::new(1.0).unwrap());
        for value in [0.49, 3.01, -1.0, 0.0, f64::NAN, f64::INFINITY] {
            let error = Speed::new(value).unwrap_err();
            assert!(
                matches!(error, SpeedError::OutsideRange { .. }),
                "velocidad {value}: {error:?}"
            );
        }
        assert_eq!(
            Speed::new(3.01).unwrap_err().to_string(),
            "la velocidad debe estar entre 0,5x y 3x; se pidió 3,01x"
        );
    }

    #[test]
    fn speed_fixed_not_combines_with_fitting_automatic() {
        let speed = Speed::new(1.5).unwrap();
        let options = ConversionOptions::new(&whatsapp()).unwrap();
        assert_eq!(options.speed(), None);
        let with_speed = options.with_speed(speed).unwrap();
        assert_eq!(with_speed.speed(), Some(speed));

        let with_fit = ConversionOptions::new(&ConversionParams {
            automatic_fitting: true,
            ..whatsapp()
        })
        .unwrap();
        assert_eq!(
            with_fit.with_speed(speed),
            Err(SpeedError::WithAutomaticFitting)
        );
    }

    #[test]
    fn priority_default_is_balance_and_not_changes_fps() {
        assert_eq!(Priority::default(), Priority::Balance);
        for (priority, fps) in [
            (Priority::Sharpness, 12),
            (Priority::Balance, 15),
            (Priority::Smoothness, 20),
        ] {
            assert_eq!(priority.default_fps().value(), fps);
        }
        let options = ConversionOptions::new(&whatsapp()).unwrap();
        assert_eq!(options.priority(), Priority::Balance);
        let smooth = options.with_priority(Priority::Smoothness);
        assert_eq!(smooth.priority(), Priority::Smoothness);
        assert_eq!(smooth.fps(), options.fps());
    }

    #[test]
    fn messages_of_error_name_option() {
        let error = ConversionOptions::new(&ConversionParams {
            max_seconds: Some(12.0),
            ..whatsapp()
        })
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("--max-seconds"), "{message}");
        assert!(message.contains("whatsapp-sticker"), "{message}");

        let error = ConversionOptions::new(&ConversionParams {
            max_mb: Some(5.0),
            ..whatsapp()
        })
        .unwrap_err();
        assert!(error.to_string().contains("--max-mb"));
    }

    #[test]
    fn rejects_fps_the_preset_does_not_allow() {
        let slow = Preset {
            frame_min_duration_ms: Some(50),
            ..WHATSAPP_STICKER
        };
        assert_eq!(slow.max_fps(), 20);
        let error = ConversionOptions::new(&ConversionParams {
            fps: Some(24),
            ..ConversionParams::for_preset(slow)
        })
        .unwrap_err();
        assert!(
            matches!(error, OptionsError::FpsNotSupported { .. }),
            "{error:?}"
        );
        assert_eq!(error.to_string(), "Esos fps no se admiten en este destino");
        let options = ConversionOptions::new(&ConversionParams {
            fps: Some(20),
            ..ConversionParams::for_preset(slow)
        })
        .unwrap();
        assert_eq!(options.fps().value(), 20);
        assert_eq!(WHATSAPP_STICKER.max_fps(), 30);
    }
}
