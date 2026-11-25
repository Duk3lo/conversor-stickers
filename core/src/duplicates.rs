use rayon::prelude::*;

use crate::clip::{Clip, ScaledClip};

const BLOCK_SIDE: usize = 32;

const DMA_MAX: u64 = 2;

pub(crate) fn remove_near_duplicates(clip: Clip) -> Clip {
    let side = clip.side();
    clip.merge_repetitions(|kept, frame| near_equal(&kept.rgba, &frame.rgba, side))
}

pub(crate) fn remove_near_duplicates_scaled(clip: ScaledClip) -> ScaledClip {
    let side = clip.side();
    clip.merge_repetitions(|kept, frame| near_equal(&kept.rgba, &frame.rgba, side))
}

fn near_equal(a: &[u8], b: &[u8], side: u32) -> bool {
    let side = side as usize;
    let blocks = side.div_ceil(BLOCK_SIDE);
    (0..blocks).into_par_iter().all(|block_row| {
        (0..blocks).all(|block_column| block_near_equal(a, b, side, block_row, block_column))
    })
}

fn block_near_equal(
    a: &[u8],
    b: &[u8],
    side: usize,
    block_row: usize,
    block_column: usize,
) -> bool {
    let rows = block_row * BLOCK_SIDE..((block_row + 1) * BLOCK_SIDE).min(side);
    let columns = block_column * BLOCK_SIDE..((block_column + 1) * BLOCK_SIDE).min(side);
    let pixels = (rows.len() * columns.len()) as u64;

    let mut differences = [0u64; 4];
    for row in rows {
        let start = (row * side + columns.start) * 4;
        let end = (row * side + columns.end) * 4;
        for (pixel_a, pixel_b) in a[start..end]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b[start..end].as_chunks::<4>().0.iter())
        {
            for channel in 0..4 {
                differences[channel] += u64::from(pixel_a[channel].abs_diff(pixel_b[channel]));
            }
        }
    }
    differences
        .iter()
        .all(|&difference| difference <= DMA_MAX * pixels)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::clip::{Frame, resample_fps};
    use crate::options::Fps;

    const SIDE: u32 = 64;

    fn gray_frame(gray: u8, duration_ms: u32) -> Frame {
        let mut rgba = vec![gray; (SIDE * SIDE * 4) as usize];
        rgba.iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 255);
        Frame {
            rgba: rgba.into(),
            duration_ms,
        }
    }

    fn frame_with_square(x: usize, y: usize, duration_ms: u32) -> Frame {
        let mut frame = gray_frame(0, duration_ms);
        for row in y..y + 8 {
            for column in x..x + 8 {
                let start = (row * SIDE as usize + column) * 4;
                Arc::make_mut(&mut frame.rgba)[start..start + 3].fill(255);
            }
        }
        frame
    }

    fn durations(clip: &Clip) -> Vec<u32> {
        clip.frames().iter().map(|f| f.duration_ms).collect()
    }

    #[test]
    fn clip_static_remains_in_frame_with_all_duration() {
        let frames = (0..30).map(|_| gray_frame(120, 67)).collect();
        let clip = Clip::new(SIDE, frames).unwrap();
        let without_duplicates = remove_near_duplicates(clip);
        assert_eq!(durations(&without_duplicates), [30 * 67]);
    }

    #[test]
    fn noise_by_below_of_threshold_count_as_duplicate() {
        let mut near_equal = gray_frame(100, 50);
        Arc::make_mut(&mut near_equal.rgba)
            .iter_mut()
            .enumerate()
            .filter(|(index, _)| index % 4 != 3)
            .for_each(|(_, channel)| *channel += 2);
        let clip = Clip::new(SIDE, vec![gray_frame(100, 50), near_equal]).unwrap();
        assert_eq!(durations(&remove_near_duplicates(clip)), [100]);

        let clip = Clip::new(SIDE, vec![gray_frame(100, 50), gray_frame(103, 50)]).unwrap();
        assert_eq!(durations(&remove_near_duplicates(clip)), [50, 50]);
    }

    #[test]
    fn object_of_8x8_that_moves_does_not_discard() {
        let frames = (0..6)
            .map(|step| frame_with_square(step * 8, 20, 67))
            .collect();
        let clip = Clip::new(SIDE, frames).unwrap();
        assert_eq!(durations(&remove_near_duplicates(clip)), [67; 6]);
    }

    #[test]
    fn compares_with_last_kept_and_not_with_previous() {
        let frames = (0..7).map(|step| gray_frame(100 + step, 10)).collect();
        let clip = Clip::new(SIDE, frames).unwrap();
        let without_duplicates = remove_near_duplicates(clip);
        let grays: Vec<u8> = without_duplicates
            .frames()
            .iter()
            .map(|f| f.rgba[0])
            .collect();
        assert_eq!(grays, [100, 103, 106]);
        assert_eq!(durations(&without_duplicates), [30, 30, 10]);
    }

    #[test]
    fn last_block_partial_also_compares() {
        let side = 40;
        let base = Frame {
            rgba: vec![0; side * side * 4].into(),
            duration_ms: 40,
        };
        let mut changed = base.clone();
        let corner = ((side - 1) * side + side - 1) * 4;
        Arc::make_mut(&mut changed.rgba)[corner..corner + 4].fill(255);
        let clip = Clip::new(side as u32, vec![base, changed]).unwrap();
        assert_eq!(durations(&remove_near_duplicates(clip)), [40, 40]);
    }

    #[test]
    fn resample_and_remove_duplicates_keeps_duration_and_8_ms() {
        let fps_origin = Fps::new(30).unwrap();
        let frames = (0..60u64)
            .map(|i| {
                let duration_ms = crate::time::frame_duration_ms(i, fps_origin);
                if i < 30 {
                    gray_frame(50, duration_ms)
                } else {
                    frame_with_square((i as usize - 30) % 7 * 8, 8, duration_ms)
                }
            })
            .collect();
        let clip = Clip::new(SIDE, frames).unwrap();
        for fps in [30, 15, 12, 10, 8, 5] {
            let resampled = resample_fps(&clip, Fps::new(fps).unwrap());
            let without_duplicates = remove_near_duplicates(resampled);
            assert_eq!(without_duplicates.total_duration_ms(), 2000, "{fps} fps");
            assert!(
                without_duplicates
                    .frames()
                    .iter()
                    .all(|f| f.duration_ms >= 8),
                "{fps} fps"
            );
            assert!(without_duplicates.frames().len() > 1, "{fps} fps");
        }
    }
}
