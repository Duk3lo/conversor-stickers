use color_quant::NeuQuant;
use rayon::prelude::*;

use crate::clip::Frame;
use crate::error::EncodingError;

pub(crate) const MIN_COLORS: u16 = 2;
pub(crate) const MAX_COLORS: u16 = 256;

const SAMPLE_FRAMES: usize = 8;

const FACTOR_SAMPLING: i32 = 10;

const OPAQUE_MIN_ALPHA: u8 = 128;

pub(crate) struct Palette {
    quantizer: NeuQuant,
    rgb: Vec<u8>,
    transparent_index: Option<u8>,
}

impl Palette {
    pub(crate) fn quantize(frames: &[Frame], colors: u16) -> Result<Palette, EncodingError> {
        if !(MIN_COLORS..=MAX_COLORS).contains(&colors) {
            return Err(EncodingError::ColorsOutsideOfRange { colors });
        }
        let has_transparency = frames.par_iter().any(|frame| has_transparency(&frame.rgba));
        let opaque_colors = usize::from(colors) - usize::from(has_transparency);

        let quantizer = NeuQuant::new(FACTOR_SAMPLING, opaque_colors, &opaque_sample(frames));
        let mut rgb = quantizer.color_map_rgb();
        let transparent_index = if has_transparency {
            rgb.extend_from_slice(&[0, 0, 0]);
            Some(
                u8::try_from(opaque_colors)
                    .map_err(|_| EncodingError::ColorsOutsideOfRange { colors })?,
            )
        } else {
            None
        };
        Ok(Palette {
            quantizer,
            rgb,
            transparent_index,
        })
    }

    pub(crate) fn rgb(&self) -> &[u8] {
        &self.rgb
    }

    pub(crate) fn transparent_index(&self) -> Option<u8> {
        self.transparent_index
    }

    pub(crate) fn index(&self, rgba: &[u8]) -> Vec<u8> {
        rgba.as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| match self.transparent_index {
                Some(transparent) if pixel[3] < OPAQUE_MIN_ALPHA => transparent,
                _ => {
                    let index = self
                        .quantizer
                        .index_of(&[pixel[0], pixel[1], pixel[2], 255]);
                    u8::try_from(index).unwrap_or(u8::MAX)
                }
            })
            .collect()
    }
}

fn has_transparency(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] < OPAQUE_MIN_ALPHA)
}

fn opaque_sample(frames: &[Frame]) -> Vec<u8> {
    let step = frames.len().div_ceil(SAMPLE_FRAMES).max(1);
    frames
        .iter()
        .step_by(step)
        .flat_map(|frame| frame.rgba.as_chunks::<4>().0.iter())
        .filter(|pixel| pixel[3] >= OPAQUE_MIN_ALPHA)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rgba: Vec<u8>) -> Frame {
        Frame {
            rgba: rgba.into(),
            duration_ms: 100,
        }
    }

    fn degraded() -> Vec<u8> {
        (0..64u32 * 64)
            .flat_map(|i| {
                [
                    (i % 64 * 4) as u8,
                    (i / 64 * 4) as u8,
                    (i % 7 * 36) as u8,
                    255,
                ]
            })
            .collect()
    }

    #[test]
    fn rejects_colors_outside_of_2_to_256() {
        let frames = [frame(degraded())];
        for colors in [0, 1, 257, 1000] {
            assert!(matches!(
                Palette::quantize(&frames, colors),
                Err(EncodingError::ColorsOutsideOfRange { .. })
            ));
        }
    }

    #[test]
    fn without_transparency_uses_all_colors_and_not_reserve_index() {
        let frames = [frame(degraded())];
        let palette = Palette::quantize(&frames, 64).unwrap();
        assert_eq!(palette.rgb().len(), 64 * 3);
        assert_eq!(palette.transparent_index(), None);
        let indices = palette.index(&frames[0].rgba);
        assert_eq!(indices.len(), 64 * 64);
        assert!(indices.iter().all(|&index| index < 64));
    }

    #[test]
    fn with_padding_reserve_last_index_for_transparent() {
        let mut rgba = degraded();
        rgba[..64 * 4 * 4].fill(0);
        let frames = [frame(rgba)];
        let palette = Palette::quantize(&frames, 16).unwrap();
        assert_eq!(palette.rgb().len(), 16 * 3);
        assert_eq!(palette.transparent_index(), Some(15));
        let indices = palette.index(&frames[0].rgba);
        assert!(indices[..64 * 4].iter().all(|&index| index == 15));
        assert!(indices[64 * 4..].iter().all(|&index| index < 15));
    }

    #[test]
    fn colors_pure_keep_with_16_colors() {
        let rgba: Vec<u8> = (0..64 * 64)
            .flat_map(|i| {
                if i % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect();
        let frames = [frame(rgba)];
        let palette = Palette::quantize(&frames, 16).unwrap();
        let indices = palette.index(&frames[0].rgba);
        let color = |index: u8| &palette.rgb()[usize::from(index) * 3..][..3];
        assert_eq!(color(indices[0]), [255, 0, 0]);
        assert_eq!(color(indices[1]), [0, 0, 255]);
    }
}
