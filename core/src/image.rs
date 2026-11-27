use tracing::info;

use crate::clip::{Clip, Frame, ScaledClip};
use crate::decoding::MAX_VIDEO_SIDE_PX;
use crate::encoding::encode_static_webp;
use crate::error::{ConversionError, EncodingError, ImageError, ScalingError};
use crate::framing_spec::FramingSpec;
use crate::options::{Fps, MaxSize};
use crate::preset::Canvas;
use crate::scaling::frame_image;
use crate::size_fitting::{
    Attempt, FittingRequest, Knob, KnobRange, Ladder, Lever, Ratio, Reduction, Step, fit_size,
};
use crate::tracking::Tracking;

const FRAME_DURATION_MS: u32 = 1;

const STATIC_QUALITY: KnobRange = KnobRange {
    height: 90,
    low: 50,
};

static STATIC_WEBP_LADDER: Ladder = Ladder {
    knob: Knob::Quality,
    requested_knob: STATIC_QUALITY.height,
    squeeze: false,
    steps: &[
        Step {
            lever: Lever::No,
            range: STATIC_QUALITY,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(3, 4), Ratio::new(1, 2)]),
            range: STATIC_QUALITY,
        },
        Step {
            lever: Lever::Size,
            range: STATIC_QUALITY,
        },
    ],
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ImageError> {
        let expected = image_bytes(width, height)?;
        if rgba.len() != expected {
            return Err(ImageError::IncorrectSize {
                expected,
                received: rgba.len(),
            });
        }
        Ok(RgbaImage {
            width,
            height,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Fast PNG, lossless and with no extra encoder: for stills shown by the interface.
    pub fn to_png(&self) -> Result<Vec<u8>, crate::EncodingError> {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(crate::EncodingError::Png)?;
        writer
            .write_image_data(&self.rgba)
            .map_err(crate::EncodingError::Png)?;
        writer.finish().map_err(crate::EncodingError::Png)?;
        Ok(png)
    }
}

pub(crate) fn image_bytes(width: u32, height: u32) -> Result<usize, ImageError> {
    let not_valid = ImageError::SideNotValid { width, height };
    let sides = 1..=MAX_VIDEO_SIDE_PX;
    if !sides.contains(&width) || !sides.contains(&height) {
        return Err(not_valid);
    }
    usize::try_from(u64::from(width) * u64::from(height) * 4).map_err(|_| not_valid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePreset {
    pub name: &'static str,
    pub description: &'static str,
    pub side: u32,
    pub max_size: MaxSize,
}

pub const WHATSAPP_STATIC_STICKER: ImagePreset = ImagePreset {
    name: "whatsapp-sticker-estatico",
    description: "Sticker estático de WhatsApp: WebP de 512x512, hasta 100 KB",
    side: 512,
    max_size: MaxSize::from_bytes(100_000),
};

pub const IMAGE_PRESETS: &[ImagePreset] = &[WHATSAPP_STATIC_STICKER];

pub fn find_image_preset(name: &str) -> Option<&'static ImagePreset> {
    IMAGE_PRESETS.iter().find(|preset| preset.name == name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageReport {
    pub preset: &'static str,
    pub bytes: Vec<u8>,
    pub max_size: MaxSize,
    pub attempt: Attempt,
    pub content_side: u32,
    pub reductions: Vec<Reduction>,
}

pub fn convert_image(
    image: &RgbaImage,
    preset: &ImagePreset,
    framing: &FramingSpec,
    tracking: Tracking<'_>,
) -> Result<ImageReport, ConversionError> {
    let fitting_error = |error| ConversionError::from_image_fitting(preset.name, error);
    if tracking.cancelled() {
        return Err(ConversionError::Cancelled);
    }
    let clip = frame(image, framing, preset.side).map_err(|error| fitting_error(error.into()))?;
    let request = FittingRequest {
        ladder: &STATIC_WEBP_LADDER,
        canvas: Canvas::Exact(preset.side),
        fps: Fps::DEFAULT,
        max_size: preset.max_size,
        speed: 1.0,
    };
    let fitting = fit_size(&clip, &request, tracking, encode).map_err(fitting_error)?;
    info!(
        preset = preset.name,
        size_bytes = fitting.bytes.len(),
        attempts = fitting.complete_encodings,
        "imagen convertida"
    );
    Ok(ImageReport {
        preset: preset.name,
        max_size: preset.max_size,
        content_side: fitting.attempt.framing().content_side(),
        attempt: fitting.attempt,
        bytes: fitting.bytes,
        reductions: fitting.reductions,
    })
}

fn frame(image: &RgbaImage, framing: &FramingSpec, side: u32) -> Result<Clip, ScalingError> {
    let sides = (image.width, image.height);
    let rgba = frame_image(&image.rgba, sides, side, framing)?;
    let frame = Frame {
        rgba,
        duration_ms: FRAME_DURATION_MS,
    };
    Ok(Clip::new(side, vec![frame])?)
}

fn encode(clip: &ScaledClip, attempt: &Attempt) -> Result<Vec<u8>, EncodingError> {
    let [frame] = clip.frames() else {
        return Err(EncodingError::ContainerWebPNotValid);
    };
    let quality = u8::try_from(attempt.value).unwrap_or(u8::MAX);
    encode_static_webp(&frame.rgba, clip.side(), quality)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use webp_animation::Decoder;

    use super::*;
    use crate::error::{ConversionError, FramingError};
    use crate::framing_spec::FramingMode;
    use crate::tracking::Tracking;

    const RED: [u8; 4] = [230, 20, 20, 255];
    const GREEN: [u8; 4] = [20, 230, 20, 255];
    const BLUE: [u8; 4] = [20, 20, 230, 255];

    fn image(width: u32, height: u32, color: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        let mut rgba = Vec::new();
        for y in 0..height {
            for x in 0..width {
                rgba.extend(color(x, y));
            }
        }
        RgbaImage::new(width, height, rgba).unwrap()
    }

    fn horizontal() -> RgbaImage {
        image(900, 300, |x, _| [RED, GREEN, BLUE][(x / 300) as usize])
    }

    fn vertical() -> RgbaImage {
        image(200, 400, |_, _| RED)
    }

    fn noise() -> RgbaImage {
        image(512, 512, |x, y| {
            let mut seed = (x + y * 512 + 1).wrapping_mul(0x9e37_79b9);
            seed ^= seed >> 13;
            seed = seed.wrapping_mul(0x5bd1_e995);
            seed ^= seed >> 15;
            let [r, g, b, _] = seed.to_le_bytes();
            [r, g, b, 255]
        })
    }

    fn pixel(rgba: &[u8], x: usize, y: usize) -> [u8; 4] {
        rgba[(y * 512 + x) * 4..][..4].try_into().unwrap()
    }

    fn similar(a: [u8; 4], b: [u8; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) < 30)
    }

    fn check_sticker(report: &ImageReport) -> Vec<u8> {
        let webp = &report.bytes;
        assert!(webp.starts_with(b"RIFF") && &webp[8..12] == b"WEBP");
        for animation in [b"ANIM", b"ANMF"] {
            let mut rest = &webp[12..];
            while let Some((code, _, next)) = crate::encoding::read_chunk(rest) {
                assert_ne!(code, animation);
                rest = next;
            }
        }
        assert!(webp.len() <= 100_000, "{} B", webp.len());
        let decoder = Decoder::new(webp).unwrap();
        assert_eq!(decoder.dimensions(), (512, 512));
        let frames: Vec<_> = decoder.into_iter().collect();
        assert_eq!(frames.len(), 1);
        frames[0].data().to_vec()
    }

    fn convert(image: &RgbaImage, framing: &FramingSpec) -> ImageReport {
        convert_image(image, &WHATSAPP_STATIC_STICKER, framing, Tracking::none()).unwrap()
    }

    #[test]
    fn crop_horizontal_keeps_band_center() {
        let clip = frame(&horizontal(), &FramingSpec::default(), 512).unwrap();
        let rgba = &clip.frames()[0].rgba;
        for (x, y) in [(8, 8), (256, 256), (503, 503)] {
            assert!(similar(pixel(rgba, x, y), GREEN), "({x}, {y})");
        }
    }

    #[test]
    fn crop_vertical_keeps_center() {
        let vertical = image(300, 900, |_, y| [RED, GREEN, BLUE][(y / 300) as usize]);
        let clip = frame(&vertical, &FramingSpec::default(), 512).unwrap();
        let rgba = &clip.frames()[0].rgba;
        for (x, y) in [(8, 8), (256, 256), (503, 503)] {
            assert!(similar(pixel(rgba, x, y), GREEN), "({x}, {y})");
        }
    }

    #[test]
    fn crop_off_centre_picks_the_requested_side() {
        let left = FramingSpec::new(FramingMode::Crop, 0.0, 0.5, 1.0).unwrap();
        let clip = frame(&horizontal(), &left, 512).unwrap();
        assert!(similar(pixel(&clip.frames()[0].rgba, 256, 256), RED));
        let right = FramingSpec::new(FramingMode::Crop, 1.0, 0.5, 1.0).unwrap();
        let clip = frame(&horizontal(), &right, 512).unwrap();
        assert!(similar(pixel(&clip.frames()[0].rgba, 256, 256), BLUE));
    }

    #[test]
    fn zoom_2_crops_a_quarter_of_the_area() {
        let quadrants = image(400, 400, |x, y| match (x < 200, y < 200) {
            (true, true) => RED,
            (false, true) => GREEN,
            (true, false) => BLUE,
            (false, false) => [230, 230, 20, 255],
        });
        let zoom = FramingSpec::new(FramingMode::Crop, 0.25, 0.25, 2.0).unwrap();
        let clip = frame(&quadrants, &zoom, 512).unwrap();
        let rgba = &clip.frames()[0].rgba;
        for (x, y) in [(8, 8), (256, 256), (503, 503)] {
            assert!(similar(pixel(rgba, x, y), RED), "({x}, {y})");
        }
    }

    #[test]
    fn invalid_framing_is_rejected() {
        assert_eq!(
            FramingSpec::new(FramingMode::Crop, 0.5, 0.5, 0.5),
            Err(FramingError::NotValid)
        );
    }

    #[test]
    fn fit_vertical_centers_over_transparency() {
        let clip = frame(&vertical(), &FramingSpec::fit(), 512).unwrap();
        let rgba = &clip.frames()[0].rgba;
        for (x, y) in [(0, 0), (127, 256), (384, 256), (511, 511)] {
            assert_eq!(pixel(rgba, x, y), [0; 4], "({x}, {y})");
        }
        for (x, y) in [(128, 0), (256, 256), (383, 511)] {
            assert!(similar(pixel(rgba, x, y), RED), "({x}, {y})");
        }
    }

    #[test]
    fn sides_ends_frame_without_panic() {
        for (width, height) in [(1, 1), (16_384, 1), (1, 16_384), (513, 512)] {
            let image = image(width, height, |_, _| BLUE);
            for framing in [FramingSpec::default(), FramingSpec::fit()] {
                let clip = frame(&image, &framing, 512).unwrap();
                assert_eq!(clip.side(), 512, "{width}x{height} {framing:?}");
            }
        }
        assert!(frame(&vertical(), &FramingSpec::fit(), 0).is_err());
    }

    #[test]
    fn sticker_meets_preset_without_reductions() {
        let report = convert(&horizontal(), &FramingSpec::default());
        let rgba = check_sticker(&report);
        assert!(similar(pixel(&rgba, 256, 256), GREEN));
        assert!(report.reductions.is_empty(), "{:?}", report.reductions);
        assert_eq!(report.preset, "whatsapp-sticker-estatico");
        assert_eq!(report.content_side, 512);
        assert_eq!(report.attempt.value, 90);
        assert_eq!(report.max_size.bytes(), 100_000);
    }

    #[test]
    fn pixels_transparent_of_fit_keep_alpha_0() {
        let rgba = check_sticker(&convert(&vertical(), &FramingSpec::fit()));
        for y in 0..512 {
            for x in (0..128).chain(384..512) {
                assert_eq!(pixel(&rgba, x, y)[3], 0, "({x}, {y})");
            }
        }
        assert!(similar(pixel(&rgba, 256, 256), RED));
    }

    #[test]
    fn noise_force_ladder_and_finishes_inside_of_size() {
        let report = convert(&noise(), &FramingSpec::default());
        check_sticker(&report);
        assert!(!report.reductions.is_empty());
        assert!(
            !report
                .reductions
                .iter()
                .any(|reduction| matches!(reduction, Reduction::Fps { .. })),
            "una imagen fija no baja fps"
        );
    }

    #[test]
    fn if_not_fits_error_speaks_of_image_and_not_of_seconds() {
        let tiny = ImagePreset {
            max_size: MaxSize::from_bytes(2000),
            ..WHATSAPP_STATIC_STICKER
        };
        let error =
            convert_image(&noise(), &tiny, &FramingSpec::default(), Tracking::none()).unwrap_err();
        let message = error.to_string();
        assert!(
            matches!(
                error,
                ConversionError::DoesNotFitInSize {
                    suggested_seconds: None,
                    fixed_image: true,
                    ..
                }
            ),
            "{error:?}"
        );
        for text in ["fps", "--start", "duración", "--max-seconds"] {
            assert!(!message.contains(text), "{message}");
        }
        assert!(message.contains("menos detalle"), "{message}");
    }

    #[test]
    fn image_returns_same_bytes_with_thread() {
        let thread = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| convert(&noise(), &FramingSpec::fit()));
        assert_eq!(thread, convert(&noise(), &FramingSpec::fit()));
    }

    #[test]
    fn convert_image_can_cancel() {
        let cancellation = AtomicBool::new(true);
        let result = convert_image(
            &vertical(),
            &WHATSAPP_STATIC_STICKER,
            &FramingSpec::default(),
            Tracking::new(&|_| {}, &cancellation),
        );
        assert!(matches!(result, Err(ConversionError::Cancelled)));
    }

    #[test]
    fn ladder_static_not_low_fps() {
        assert!(
            STATIC_WEBP_LADDER
                .steps
                .iter()
                .all(|step| !matches!(step.lever, Lever::Fps(_)))
        );
    }

    #[test]
    fn image_has_4_bytes_by_pixel() {
        let image = RgbaImage::new(3, 2, vec![7; 24]).unwrap();
        assert_eq!((image.width(), image.height()), (3, 2));
        assert_eq!(image.rgba(), [7; 24]);
        for received in [0, 23, 25] {
            assert_eq!(
                RgbaImage::new(3, 2, vec![0; received]),
                Err(ImageError::IncorrectSize {
                    expected: 24,
                    received
                })
            );
        }
    }

    #[test]
    fn image_has_no_sides_0_nor_over_16384() {
        for (width, height) in [
            (0, 2),
            (2, 0),
            (16_385, 1),
            (1, 16_385),
            (u32::MAX, u32::MAX),
        ] {
            assert_eq!(
                RgbaImage::new(width, height, Vec::new()),
                Err(ImageError::SideNotValid { width, height })
            );
        }
    }

    #[test]
    fn sticker_static_of_whatsapp_is_of_512_and_100_kb() {
        let preset = WHATSAPP_STATIC_STICKER;
        assert_eq!(preset.name, "whatsapp-sticker-estatico");
        assert_eq!(preset.side, 512);
        assert_eq!(preset.max_size.bytes(), 100_000);
        assert_eq!(IMAGE_PRESETS, [preset]);
        assert_eq!(find_image_preset(preset.name), Some(&IMAGE_PRESETS[0]));
        assert_eq!(find_image_preset("whatsapp-sticker"), None);
        assert_eq!(crate::preset::find_preset(preset.name), None);
    }
}
