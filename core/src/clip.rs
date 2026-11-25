use std::sync::Arc;

use crate::error::ClipError;
use crate::options::Fps;
use crate::time::frame_mark_ms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub rgba: Arc<[u8]>,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    side: u32,
    frames: Vec<Frame>,
}

impl Clip {
    pub fn new(side: u32, frames: Vec<Frame>) -> Result<Self, ClipError> {
        validate_frames(side, &frames)?;
        Ok(Clip { side, frames })
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn total_duration_ms(&self) -> u64 {
        total_duration_ms(&self.frames)
    }

    /// `replace` must return frames of the same side, in the same number and with the same durations.
    pub(crate) fn replace_pictures(mut self, replace: impl FnOnce(&[Frame]) -> Vec<Frame>) -> Clip {
        self.frames = replace(&self.frames);
        self
    }

    pub(crate) fn merge_repetitions(
        mut self,
        is_repetition: impl Fn(&Frame, &Frame) -> bool,
    ) -> Clip {
        merge_repetitions(&mut self.frames, is_repetition);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaledClip {
    side: u32,
    frames: Vec<Frame>,
}

impl ScaledClip {
    pub fn new(side: u32, frames: Vec<Frame>) -> Result<Self, ClipError> {
        validate_frames(side, &frames)?;
        Ok(ScaledClip { side, frames })
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn total_duration_ms(&self) -> u64 {
        total_duration_ms(&self.frames)
    }

    pub(crate) fn merge_repetitions(
        mut self,
        is_repetition: impl Fn(&Frame, &Frame) -> bool,
    ) -> ScaledClip {
        merge_repetitions(&mut self.frames, is_repetition);
        self
    }
}

fn merge_repetitions(frames: &mut Vec<Frame>, is_repetition: impl Fn(&Frame, &Frame) -> bool) {
    frames.dedup_by(|frame, kept| {
        if is_repetition(kept, frame) {
            kept.duration_ms = kept.duration_ms.saturating_add(frame.duration_ms);
            true
        } else {
            false
        }
    });
}

pub(crate) fn bytes_by_frame(side: u32) -> Option<usize> {
    let side = usize::try_from(side).ok().filter(|&side| side > 0)?;
    side.checked_mul(side)?.checked_mul(4)
}

pub(crate) fn resample_fps(clip: &Clip, fps: Fps) -> Clip {
    resample(clip, fps, 1.0, clip.total_duration_ms())
}

pub(crate) fn range_without_accelerate(clip: &Clip, fps: Fps, speed: f64, total_ms: u64) -> Clip {
    resample(clip, fps, speed, total_ms)
}

fn resample(clip: &Clip, fps: Fps, speed: f64, total_ms: u64) -> Clip {
    let stretch = |mark_ms: u64| (mark_ms as f64 * speed).floor() as u64;
    let fps_value = u64::from(fps.value());
    let number = ((total_ms * fps_value + 500) / 1000).max(1);

    let Some((first, rest)) = clip.frames.split_first() else {
        return clip.clone();
    };
    let mut frames = Vec::new();
    let mut origin = rest.iter();
    let mut current = first;
    let mut end_original_ms = u64::from(first.duration_ms);
    let mut current_end_ms = stretch(end_original_ms);
    for index in 0..number {
        let start_ms = frame_mark_ms(index, fps);
        let end_ms = if index + 1 == number {
            total_ms
        } else {
            frame_mark_ms(index + 1, fps)
        };
        while start_ms >= current_end_ms {
            match origin.next() {
                Some(next) => {
                    current = next;
                    end_original_ms += u64::from(next.duration_ms);
                    current_end_ms = stretch(end_original_ms);
                }
                None => break,
            }
        }
        frames.push(Frame {
            rgba: Arc::clone(&current.rgba),
            duration_ms: u32::try_from(end_ms - start_ms).unwrap_or(u32::MAX),
        });
    }
    Clip {
        side: clip.side,
        frames,
    }
}

fn validate_frames(side: u32, frames: &[Frame]) -> Result<(), ClipError> {
    let expected = bytes_by_frame(side).ok_or(ClipError::SideNotValid { side })?;
    if frames.is_empty() {
        return Err(ClipError::WithoutFrames);
    }
    for (index, frame) in frames.iter().enumerate() {
        if frame.rgba.len() != expected {
            return Err(ClipError::IncorrectFrameSize {
                index,
                expected,
                real: frame.rgba.len(),
            });
        }
        if frame.duration_ms == 0 {
            return Err(ClipError::FrameWithoutDuration { index });
        }
    }
    Ok(())
}

fn total_duration_ms(frames: &[Frame]) -> u64 {
    frames.iter().map(|f| u64::from(f.duration_ms)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(side: usize, duration_ms: u32) -> Frame {
        Frame {
            rgba: vec![0; side * side * 4].into(),
            duration_ms,
        }
    }

    #[test]
    fn clip_valid_keeps_frames_and_sum_durations() {
        let clip = Clip::new(4, vec![frame(4, 67), frame(4, 66)]).unwrap();
        assert_eq!(clip.side(), 4);
        assert_eq!(clip.frames().len(), 2);
        assert_eq!(clip.total_duration_ms(), 133);

        let scaling = ScaledClip::new(4, vec![frame(4, 100)]).unwrap();
        assert_eq!(scaling.side(), 4);
        assert_eq!(scaling.total_duration_ms(), 100);
    }

    #[test]
    fn rejects_clip_without_frames() {
        assert_eq!(Clip::new(4, vec![]), Err(ClipError::WithoutFrames));
        assert_eq!(ScaledClip::new(4, vec![]), Err(ClipError::WithoutFrames));
    }

    #[test]
    fn rejects_zero_side() {
        assert_eq!(
            Clip::new(0, vec![frame(0, 10)]),
            Err(ClipError::SideNotValid { side: 0 })
        );
    }

    #[test]
    fn rejects_frames_with_different_side() {
        assert_eq!(
            Clip::new(4, vec![frame(4, 10), frame(3, 10)]),
            Err(ClipError::IncorrectFrameSize {
                index: 1,
                expected: 64,
                real: 36
            })
        );
    }

    #[test]
    fn rejects_frames_without_duration() {
        assert_eq!(
            ScaledClip::new(2, vec![frame(2, 10), frame(2, 0)]),
            Err(ClipError::FrameWithoutDuration { index: 1 })
        );
    }

    fn numbered_clip(durations_ms: &[u32]) -> Clip {
        let frames = durations_ms
            .iter()
            .enumerate()
            .map(|(index, &duration_ms)| Frame {
                rgba: vec![index as u8, 0, 0, 255].into(),
                duration_ms,
            })
            .collect();
        Clip::new(1, frames).unwrap()
    }

    fn clip_a_fps(fps: u32, number: u64) -> Clip {
        let fps = Fps::new(fps).unwrap();
        let durations: Vec<u32> = (0..number)
            .map(|i| crate::time::frame_duration_ms(i, fps))
            .collect();
        numbered_clip(&durations)
    }

    fn red(clip: &Clip) -> Vec<u8> {
        clip.frames().iter().map(|f| f.rgba[0]).collect()
    }

    #[test]
    fn resample_to_same_fps_not_changes_clip_nor_backup_pixels() {
        let clip = clip_a_fps(15, 150);
        let resampled = resample_fps(&clip, Fps::new(15).unwrap());
        assert_eq!(resampled, clip);
        for (new, original) in resampled.frames().iter().zip(clip.frames()) {
            assert!(Arc::ptr_eq(&new.rgba, &original.rgba));
        }
    }

    #[test]
    fn resample_keeps_duration_total_exact() {
        for (fps_origin, number) in [(15, 150), (30, 91), (15, 45), (12, 7), (5, 1)] {
            let clip = clip_a_fps(fps_origin, number);
            for fps in Fps::MIN..=Fps::MAX {
                let resampled = resample_fps(&clip, Fps::new(fps).unwrap());
                assert_eq!(
                    resampled.total_duration_ms(),
                    clip.total_duration_ms(),
                    "{fps_origin} fps x {number} -> {fps} fps"
                );
                assert_eq!(resampled.side(), clip.side());
                let min = resampled
                    .frames()
                    .iter()
                    .map(|f| f.duration_ms)
                    .min()
                    .unwrap();
                assert!(min >= 8, "{fps_origin} -> {fps}: frame de {min} ms");
            }
        }
    }

    #[test]
    fn range_without_accelerate_to_speed_1_is_resample_start() {
        let clip = clip_a_fps(15, 150);
        let start = Clip::new(1, clip.frames()[..75].to_vec()).unwrap();
        let fps = Fps::new(10).unwrap();
        assert_eq!(
            range_without_accelerate(&clip, fps, 1.0, 5000),
            resample_fps(&start, fps)
        );
    }

    #[test]
    fn range_without_accelerate_undoes_acceleration() {
        let clip = clip_a_fps(10, 10);
        let range = range_without_accelerate(&clip, Fps::new(5).unwrap(), 2.0, 1600);
        assert_eq!(red(&range), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(range.frames().iter().all(|f| f.duration_ms == 200));
        let range = range_without_accelerate(&clip, Fps::new(5).unwrap(), 1.5, 1000);
        assert_eq!(red(&range), [0, 1, 2, 4, 5]);
        assert_eq!(range.total_duration_ms(), 1000);
    }

    #[test]
    fn resample_of_15_to_10_fps_takes_frame_visible_in_each_mark() {
        let clip = clip_a_fps(15, 15);
        let resampled = resample_fps(&clip, Fps::new(10).unwrap());
        assert_eq!(red(&resampled), [0, 1, 3, 4, 6, 7, 9, 10, 12, 13]);
        let durations: Vec<u32> = resampled.frames().iter().map(|f| f.duration_ms).collect();
        assert_eq!(durations, [100; 10]);
    }

    #[test]
    fn resample_clip_very_short_returns_only_frame_with_all_duration() {
        let clip = numbered_clip(&[20]);
        let resampled = resample_fps(&clip, Fps::new(8).unwrap());
        assert_eq!(resampled, clip);
    }

    #[test]
    fn bytes_by_frame_is_side_by_side_by_4() {
        assert_eq!(bytes_by_frame(512), Some(512 * 512 * 4));
        assert_eq!(bytes_by_frame(0), None);
    }
}
