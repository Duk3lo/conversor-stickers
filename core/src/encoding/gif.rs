use gif::{DisposalMethod, Encoder, Frame, Repeat};
use rayon::prelude::*;

use crate::clip::ScaledClip;
use crate::error::EncodingError;
use crate::palette::Palette;

const MIN_DELAY_CS: u16 = 2;

pub trait GifEncoder: Send + Sync {
    fn encode(&self, clip: &ScaledClip, colors: u16) -> Result<Vec<u8>, EncodingError>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GifNeuQuant;

impl GifEncoder for GifNeuQuant {
    fn encode(&self, clip: &ScaledClip, colors: u16) -> Result<Vec<u8>, EncodingError> {
        let side = u16::try_from(clip.side())
            .map_err(|_| EncodingError::SideGifNotValid { side: clip.side() })?;
        let palette = Palette::quantize(clip.frames(), colors)?;
        let delays = delays_cs(clip)?;

        let frames: Vec<Frame<'static>> = clip
            .frames()
            .par_iter()
            .zip(delays.par_iter())
            .map(|(frame, &delay)| {
                let mut frame = Frame::from_indexed_pixels(
                    side,
                    side,
                    palette.index(&frame.rgba),
                    palette.transparent_index(),
                );
                frame.delay = delay;
                frame.dispose = DisposalMethod::Background;
                frame.make_lzw_pre_encoded();
                frame
            })
            .collect();

        let mut encoder =
            Encoder::new(Vec::new(), side, side, palette.rgb()).map_err(EncodingError::Gif)?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(EncodingError::Gif)?;
        for frame in &frames {
            encoder
                .write_lzw_pre_encoded_frame(frame)
                .map_err(EncodingError::Gif)?;
        }
        encoder.into_inner().map_err(EncodingError::Gif)
    }
}

fn delays_cs(clip: &ScaledClip) -> Result<Vec<u16>, EncodingError> {
    let mut end_ms: u64 = 0;
    let mut emitted_cs: u64 = 0;
    clip.frames()
        .iter()
        .map(|frame| {
            end_ms += u64::from(frame.duration_ms);
            let target_cs = (end_ms + 5) / 10;
            let delay = target_cs
                .saturating_sub(emitted_cs)
                .max(u64::from(MIN_DELAY_CS));
            emitted_cs += delay;
            u16::try_from(delay).map_err(|_| EncodingError::AnimationTooLong {
                duration_ms: clip.total_duration_ms(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;
    use crate::clip::Frame;

    const SIDE: u32 = 64;

    fn degraded_clip(durations_ms: &[u32]) -> ScaledClip {
        let frames = durations_ms
            .iter()
            .enumerate()
            .map(|(step, &duration_ms)| {
                let rgba = (0..SIDE * SIDE)
                    .flat_map(|i| {
                        let (x, y) = (i % SIDE, i / SIDE);
                        let step = step as u32;
                        [
                            ((x + step) * 4) as u8,
                            (y * 4) as u8,
                            ((x * y + step * 13) % 256) as u8,
                            255,
                        ]
                    })
                    .collect();
                Frame { rgba, duration_ms }
            })
            .collect();
        ScaledClip::new(SIDE, frames).unwrap()
    }

    struct RereadGif {
        dimensions: (u16, u16),
        delays_cs: Vec<u16>,
        colors: HashSet<[u8; 4]>,
        transparent_by_frame: Vec<usize>,
    }

    fn reread(bytes: &[u8]) -> RereadGif {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(bytes).unwrap();
        let dimensions = (decoder.width(), decoder.height());
        let mut reread = RereadGif {
            dimensions,
            delays_cs: Vec::new(),
            colors: HashSet::new(),
            transparent_by_frame: Vec::new(),
        };
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            reread.delays_cs.push(frame.delay);
            let mut transparent = 0;
            for pixel in frame.buffer.as_chunks::<4>().0.iter() {
                reread.colors.insert(*pixel);
                transparent += usize::from(pixel[3] == 0);
            }
            reread.transparent_by_frame.push(transparent);
        }
        reread
    }

    #[test]
    fn forward_and_lap_keeps_dimensions_frames_and_colors_max() {
        let clip = degraded_clip(&[67; 6]);
        for colors in [256, 64, 16, 2] {
            let gif = GifNeuQuant.encode(&clip, colors).unwrap();
            let reread = reread(&gif);
            assert_eq!(reread.dimensions, (64, 64));
            assert_eq!(reread.delays_cs.len(), 6);
            assert!(
                reread.colors.len() <= usize::from(colors),
                "{colors} colores pedidos, {} en el archivo",
                reread.colors.len()
            );
        }
    }

    #[test]
    fn sum_of_delays_matches_with_total_and_none_low_of_2_cs() {
        for durations in [
            vec![67, 66, 67, 67, 66, 67],
            vec![33, 34, 33, 33, 34, 33, 33],
            vec![125; 8],
            vec![8, 8, 8, 50, 8, 300],
        ] {
            let clip = degraded_clip(&durations);
            let reread = reread(&GifNeuQuant.encode(&clip, 64).unwrap());
            assert_eq!(reread.delays_cs.len(), durations.len());
            assert!(
                reread.delays_cs.iter().all(|&cs| cs >= 2),
                "{:?}",
                reread.delays_cs
            );
            let total_ms: i64 = reread.delays_cs.iter().map(|&cs| i64::from(cs) * 10).sum();
            let expected_ms = clip.total_duration_ms() as i64;
            assert!(
                (total_ms - expected_ms).abs() <= 10,
                "{durations:?}: {total_ms} ms frente a {expected_ms} ms"
            );
        }
    }

    #[test]
    fn with_64_colors_weighs_less_that_with_256() {
        let clip = degraded_clip(&[67; 6]);
        let with_256 = GifNeuQuant.encode(&clip, 256).unwrap().len();
        let with_64 = GifNeuQuant.encode(&clip, 64).unwrap().len();
        assert!(
            with_64 < with_256,
            "64 colores: {with_64} B, 256 colores: {with_256} B"
        );
    }

    #[test]
    fn padding_transparent_keeps() {
        let mut clip = degraded_clip(&[67; 3]).frames().to_vec();
        for frame in &mut clip {
            Arc::make_mut(&mut frame.rgba)[..(SIDE * 8 * 4) as usize].fill(0);
        }
        let clip = ScaledClip::new(SIDE, clip).unwrap();
        let reread = reread(&GifNeuQuant.encode(&clip, 64).unwrap());
        assert_eq!(reread.transparent_by_frame, [(SIDE * 8) as usize; 3]);
    }

    #[test]
    fn same_input_returns_same_bytes() {
        let clip = degraded_clip(&[67; 5]);
        assert_eq!(
            GifNeuQuant.encode(&clip, 128).unwrap(),
            GifNeuQuant.encode(&clip, 128).unwrap()
        );
    }

    #[test]
    fn returns_same_bytes_with_thread_that_with_several() {
        let clip = degraded_clip(&[67; 12]);
        let thread = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(|| GifNeuQuant.encode(&clip, 128).unwrap());
        let several = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap()
            .install(|| GifNeuQuant.encode(&clip, 128).unwrap());
        assert_eq!(thread, several);
    }

    #[test]
    fn rejects_colors_outside_of_range() {
        let clip = degraded_clip(&[67]);
        assert!(matches!(
            GifNeuQuant.encode(&clip, 1),
            Err(EncodingError::ColorsOutsideOfRange { colors: 1 })
        ));
    }

    #[test]
    fn encoder_uses_as_object_of_trait() {
        let encoder: Box<dyn GifEncoder> = Box::new(GifNeuQuant);
        assert!(encoder.encode(&degraded_clip(&[67, 67]), 32).is_ok());
    }
}
