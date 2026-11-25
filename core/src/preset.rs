use std::fmt;

use crate::options::{Fps, MaxSize, Priority};
use crate::size_fitting::{
    FIT_ANYWAY_PALETTE_LADDER, FIT_ANYWAY_WEBP_LADDER, FLUID_PALETTE_LADDER, FLUID_WEBP_LADDER,
    Ladder, PALETTE_LADDER, SHARPNESS_PALETTE_LADDER, SHARPNESS_WEBP_LADDER,
    SMOOTHNESS_PALETTE_LADDER, SMOOTHNESS_WEBP_LADDER, WEBP_LADDER,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    AnimatedWebp,
    Gif,
    Apng,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::AnimatedWebp => "webp",
            Format::Gif => "gif",
            Format::Apng => "png",
        }
    }

    pub(crate) fn ladder(self, priority: Priority) -> &'static Ladder {
        match (self, priority) {
            (Format::AnimatedWebp, Priority::Sharpness) => &SHARPNESS_WEBP_LADDER,
            (Format::AnimatedWebp, Priority::Balance) => &WEBP_LADDER,
            (Format::AnimatedWebp, Priority::Smoothness) => &SMOOTHNESS_WEBP_LADDER,
            (Format::AnimatedWebp, Priority::FitAnyway) => &FIT_ANYWAY_WEBP_LADDER,
            (Format::AnimatedWebp, Priority::Fluid) => &FLUID_WEBP_LADDER,
            (Format::Gif | Format::Apng, Priority::Sharpness) => &SHARPNESS_PALETTE_LADDER,
            (Format::Gif | Format::Apng, Priority::Balance) => &PALETTE_LADDER,
            (Format::Gif | Format::Apng, Priority::Smoothness) => &SMOOTHNESS_PALETTE_LADDER,
            (Format::Gif | Format::Apng, Priority::FitAnyway) => &FIT_ANYWAY_PALETTE_LADDER,
            (Format::Gif | Format::Apng, Priority::Fluid) => &FLUID_PALETTE_LADDER,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Canvas {
    Exact(u32),
    Max(u32),
}

impl Canvas {
    pub fn side(self) -> u32 {
        match self {
            Canvas::Exact(side) | Canvas::Max(side) => side,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaxMbRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    pub format: Format,
    pub canvas: Canvas,
    pub max_size: MaxSize,
    pub max_duration_s: f64,
    pub frame_min_duration_ms: Option<u32>,
    pub range_max_mb: Option<MaxMbRange>,
}

impl Preset {
    /// Highest fps whose frames last at least the minimum duration the destination accepts.
    pub fn max_fps(&self) -> u32 {
        match self.frame_min_duration_ms {
            Some(min_ms) if min_ms > 0 => (1000 / min_ms).min(Fps::MAX),
            _ => Fps::MAX,
        }
    }
}

pub const WHATSAPP_STICKER: Preset = Preset {
    name: "whatsapp-sticker",
    description: "Sticker animado de WhatsApp: WebP de 512x512, hasta 500 KB y 10 s",
    format: Format::AnimatedWebp,
    canvas: Canvas::Exact(512),
    max_size: MaxSize::from_bytes(500_000),
    max_duration_s: 10.0,
    frame_min_duration_ms: Some(8),
    range_max_mb: None,
};

pub const DISCORD_GIF: Preset = Preset {
    name: "discord-gif",
    description: "GIF para Discord: lado de hasta 480 px, 10 MB por defecto (--max-mb) y 30 s",
    format: Format::Gif,
    canvas: Canvas::Max(480),
    max_size: MaxSize::from_bytes(10_000_000),
    max_duration_s: 30.0,
    frame_min_duration_ms: None,
    range_max_mb: Some(MaxMbRange {
        min: 0.5,
        max: 20.0,
    }),
};

pub const DISCORD_EMOJI: Preset = Preset {
    name: "discord-emoji",
    description: "Emoji animado de Discord: GIF de 128x128, hasta 256 KB y 10 s",
    format: Format::Gif,
    canvas: Canvas::Exact(128),
    max_size: MaxSize::from_bytes(256_000),
    max_duration_s: 10.0,
    frame_min_duration_ms: None,
    range_max_mb: None,
};

pub const DISCORD_STICKER: Preset = Preset {
    name: "discord-sticker",
    description: "Sticker de Discord: APNG de 320x320, hasta 512 KB y 5 s",
    format: Format::Apng,
    canvas: Canvas::Exact(320),
    max_size: MaxSize::from_bytes(512_000),
    max_duration_s: 5.0,
    frame_min_duration_ms: None,
    range_max_mb: None,
};

pub const PRESETS: &[Preset] = &[
    WHATSAPP_STICKER,
    DISCORD_GIF,
    DISCORD_EMOJI,
    DISCORD_STICKER,
];

pub fn find_preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.name == name)
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whatsapp_sticker_is_webp_of_512_exact_500_kb_and_10_s() {
        let preset = WHATSAPP_STICKER;
        assert_eq!(preset.format, Format::AnimatedWebp);
        assert_eq!(preset.canvas, Canvas::Exact(512));
        assert_eq!(preset.max_size.bytes(), 500_000);
        assert_eq!(preset.max_duration_s, 10.0);
        assert_eq!(preset.frame_min_duration_ms, Some(8));
        assert_eq!(preset.range_max_mb, None);
    }

    #[test]
    fn discord_gif_is_gif_of_side_max_480_and_10_mb_configurable_until_20() {
        let preset = DISCORD_GIF;
        assert_eq!(preset.format, Format::Gif);
        assert_eq!(preset.canvas, Canvas::Max(480));
        assert_eq!(preset.max_size.bytes(), 10_000_000);
        assert_eq!(preset.max_duration_s, 30.0);
        assert_eq!(preset.frame_min_duration_ms, None);
        assert_eq!(
            preset.range_max_mb,
            Some(MaxMbRange {
                min: 0.5,
                max: 20.0
            })
        );
    }

    #[test]
    fn discord_emoji_is_gif_of_128_exact_256_kb_and_10_s() {
        let preset = DISCORD_EMOJI;
        assert_eq!(preset.format, Format::Gif);
        assert_eq!(preset.canvas, Canvas::Exact(128));
        assert_eq!(preset.max_size.bytes(), 256_000);
        assert_eq!(preset.max_duration_s, 10.0);
        assert_eq!(preset.frame_min_duration_ms, None);
        assert_eq!(preset.range_max_mb, None);
    }

    #[test]
    fn discord_sticker_is_apng_of_320_exact_512_kb_and_5_s() {
        let preset = DISCORD_STICKER;
        assert_eq!(preset.format, Format::Apng);
        assert_eq!(preset.canvas, Canvas::Exact(320));
        assert_eq!(preset.max_size.bytes(), 512_000);
        assert_eq!(preset.max_duration_s, 5.0);
        assert_eq!(preset.frame_min_duration_ms, None);
        assert_eq!(preset.range_max_mb, None);
    }

    #[test]
    fn table_of_presets_is_consistent() {
        for (index, preset) in PRESETS.iter().enumerate() {
            assert!(
                PRESETS[..index]
                    .iter()
                    .all(|other| other.name != preset.name),
                "nombre repetido: {preset}"
            );
            assert!(!preset.description.is_empty(), "{preset}");
            assert!(preset.canvas.side() > 0, "{preset}");
            assert!(preset.max_size.bytes() > 0, "{preset}");
            assert!(preset.max_duration_s > 0.0, "{preset}");
            if let Some(range) = preset.range_max_mb {
                let default_mb = preset.max_size.bytes() as f64 / 1_000_000.0;
                assert!(0.0 < range.min && range.min <= range.max, "{preset}");
                assert!(
                    (range.min..=range.max).contains(&default_mb),
                    "{preset}: el peso por defecto queda fuera de --max-mb"
                );
            }
        }
    }

    #[test]
    fn searches_presets_by_name() {
        assert_eq!(find_preset("whatsapp-sticker"), Some(&WHATSAPP_STICKER));
        assert_eq!(find_preset("discord-gif"), Some(&DISCORD_GIF));
        assert_eq!(find_preset("discord-emoji"), Some(&DISCORD_EMOJI));
        assert_eq!(find_preset("discord-sticker"), Some(&DISCORD_STICKER));
        assert_eq!(find_preset("Discord-GIF"), None);
        assert_eq!(find_preset(""), None);
    }

    #[test]
    fn side_of_canvas_is_max_in_both_kinds() {
        assert_eq!(Canvas::Exact(512).side(), 512);
        assert_eq!(Canvas::Max(480).side(), 480);
    }

    #[test]
    fn extension_matches_format() {
        assert_eq!(Format::AnimatedWebp.extension(), "webp");
        assert_eq!(Format::Gif.extension(), "gif");
        assert_eq!(Format::Apng.extension(), "png");
    }

    #[test]
    fn each_format_and_priority_have_its_ladder() {
        use Priority::{Balance, FitAnyway, Fluid, Sharpness, Smoothness};
        for (format, priority, ladder) in [
            (Format::AnimatedWebp, Sharpness, &SHARPNESS_WEBP_LADDER),
            (Format::AnimatedWebp, Balance, &WEBP_LADDER),
            (Format::AnimatedWebp, Smoothness, &SMOOTHNESS_WEBP_LADDER),
            (Format::AnimatedWebp, FitAnyway, &FIT_ANYWAY_WEBP_LADDER),
            (Format::AnimatedWebp, Fluid, &FLUID_WEBP_LADDER),
            (Format::Gif, Sharpness, &SHARPNESS_PALETTE_LADDER),
            (Format::Gif, Balance, &PALETTE_LADDER),
            (Format::Gif, Smoothness, &SMOOTHNESS_PALETTE_LADDER),
            (Format::Gif, FitAnyway, &FIT_ANYWAY_PALETTE_LADDER),
            (Format::Gif, Fluid, &FLUID_PALETTE_LADDER),
            (Format::Apng, Sharpness, &SHARPNESS_PALETTE_LADDER),
            (Format::Apng, Balance, &PALETTE_LADDER),
            (Format::Apng, Smoothness, &SMOOTHNESS_PALETTE_LADDER),
            (Format::Apng, FitAnyway, &FIT_ANYWAY_PALETTE_LADDER),
            (Format::Apng, Fluid, &FLUID_PALETTE_LADDER),
        ] {
            assert!(
                std::ptr::eq(format.ladder(priority), ladder),
                "{format:?} con {priority:?}"
            );
        }
        assert!(std::ptr::eq(
            Format::AnimatedWebp.ladder(Priority::default()),
            &WEBP_LADDER
        ));
    }

    #[test]
    fn preset_sample_with_its_name() {
        assert_eq!(WHATSAPP_STICKER.to_string(), "whatsapp-sticker");
    }
}
