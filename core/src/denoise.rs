use std::sync::Arc;

use rayon::prelude::*;

use crate::clip::{Clip, Frame};

/// Largest weight given to the previous output, out of 256: `3/4`.
const WEIGHT_MAX: i32 = 192;

/// From this difference between a channel and the previous output on, the change is motion and
/// not noise, so the channel is kept as it is.
const DIFFERENCE_MOTION: i32 = 32;

const CHUNK_BYTES: usize = 1 << 16;

/// Recursive temporal filter: each channel is pulled towards the same channel of the previous
/// output, the less the more they differ, so grain and compression noise on still areas
/// disappear (the encoder spends no bytes on them) and real motion is not touched.
/// Integer arithmetic only, so the bytes do not depend on the platform.
pub(crate) fn denoise_temporal(clip: Clip) -> Clip {
    clip.replace_pictures(|frames| {
        let mut output: Vec<Frame> = Vec::with_capacity(frames.len());
        for (index, frame) in frames.iter().enumerate() {
            let rgba = match (index.checked_sub(1), output.last()) {
                (Some(previous), Some(last))
                    if Arc::ptr_eq(&frames[previous].rgba, &frame.rgba) =>
                {
                    Arc::clone(&last.rgba)
                }
                (_, Some(last)) => filter(&last.rgba, &frame.rgba),
                _ => Arc::clone(&frame.rgba),
            };
            output.push(Frame {
                rgba,
                duration_ms: frame.duration_ms,
            });
        }
        output
    })
}

fn weight(difference: i32) -> i32 {
    let (motion, difference) = (DIFFERENCE_MOTION, difference.abs());
    if difference >= motion {
        0
    } else {
        WEIGHT_MAX * (motion * motion - difference * difference) / (motion * motion)
    }
}

fn filter(previous: &[u8], current: &[u8]) -> Arc<[u8]> {
    let mut output = current.to_vec();
    output
        .par_chunks_mut(CHUNK_BYTES)
        .zip(previous.par_chunks(CHUNK_BYTES))
        .for_each(|(output, previous)| {
            for (pixel, previous) in output
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(previous.as_chunks::<4>().0)
            {
                for channel in 0..3 {
                    let difference = i32::from(previous[channel]) - i32::from(pixel[channel]);
                    let moved = (difference * weight(difference) + 128) >> 8;
                    pixel[channel] = (i32::from(pixel[channel]) + moved) as u8;
                }
            }
        });
    output.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: u32 = 64;

    fn gray_frame(gray: u8, alpha: u8, duration_ms: u32) -> Frame {
        let pixels = (SIDE * SIDE) as usize;
        let rgba: Vec<u8> = (0..pixels)
            .flat_map(|_| [gray, gray, gray, alpha])
            .collect();
        Frame {
            rgba: rgba.into(),
            duration_ms,
        }
    }

    fn clip(frames: Vec<Frame>) -> Clip {
        Clip::new(SIDE, frames).unwrap()
    }

    #[test]
    fn first_frame_is_kept_and_durations_do_not_change() {
        let denoised = denoise_temporal(clip(vec![
            gray_frame(100, 255, 41),
            gray_frame(104, 255, 42),
            gray_frame(98, 255, 43),
        ]));
        assert_eq!(denoised.frames()[0].rgba[0], 100);
        let durations: Vec<u32> = denoised.frames().iter().map(|f| f.duration_ms).collect();
        assert_eq!(durations, [41, 42, 43]);
        assert_eq!(denoised.total_duration_ms(), 126);
    }

    #[test]
    fn flicker_on_a_still_area_is_pulled_together() {
        let grays = [100, 106, 94, 106, 94, 106];
        let frames = grays
            .iter()
            .map(|&gray| gray_frame(gray, 255, 40))
            .collect();
        let denoised = denoise_temporal(clip(frames));
        let swing = |clip: &Clip| {
            let values: Vec<u8> = clip.frames().iter().skip(1).map(|f| f.rgba[0]).collect();
            values.iter().max().unwrap() - values.iter().min().unwrap()
        };
        assert!(swing(&denoised) < 6, "{}", swing(&denoised));
    }

    #[test]
    fn real_change_passes_through_unchanged() {
        let denoised = denoise_temporal(clip(vec![
            gray_frame(30, 255, 40),
            gray_frame(200, 255, 40),
        ]));
        assert_eq!(denoised.frames()[1].rgba[0], 200);
    }

    #[test]
    fn alpha_is_never_touched() {
        let denoised =
            denoise_temporal(clip(vec![gray_frame(100, 255, 40), gray_frame(104, 7, 40)]));
        assert_eq!(denoised.frames()[1].rgba[3], 7);
    }

    #[test]
    fn repeated_picture_stays_repeated() {
        let still = gray_frame(100, 255, 40);
        let moved = gray_frame(110, 255, 40);
        let denoised = denoise_temporal(clip(vec![still.clone(), moved.clone(), moved]));
        assert!(Arc::ptr_eq(
            &denoised.frames()[1].rgba,
            &denoised.frames()[2].rgba
        ));
    }
}
