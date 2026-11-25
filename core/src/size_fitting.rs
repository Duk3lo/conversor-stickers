use std::fmt;

use rayon::prelude::*;
use tracing::{debug, warn};

use crate::clip::{Clip, Frame, ScaledClip, range_without_accelerate, resample_fps};
use crate::denoise::denoise_temporal;
use crate::duplicates::remove_near_duplicates;
use crate::error::{EncodingError, ScalingError, SizeFittingError};
use crate::options::{Fps, MaxSize};
use crate::preset::Canvas;
use crate::scaling::{Framing, scale_frames};
use crate::time::frame_mark_ms;
use crate::tracking::{Progress, Tracking};

pub(crate) const MAX_ENCODINGS_COMPLETE: u32 = 10;

const MAX_ENCODINGS_LADDER: u32 = MAX_ENCODINGS_COMPLETE - 1;

pub(crate) const MAX_VERIFICATIONS_SUGGESTION: u32 = 4;

const SUGGESTION_STEP_S: f64 = 0.5;

const MAX_ENCODINGS_COMPLETE_BY_CONFIG: u32 = 2;

const SEARCH_MAX_STEPS: u32 = 8;

const ESTIMATE_MARGIN: f64 = 0.95;

const FRAMES_BY_WINDOW: usize = 15;

const WINDOWS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    numerator: u32,
    denominator: u32,
}

impl Ratio {
    pub const WHOLE: Ratio = Ratio::new(1, 1);

    pub(crate) const fn new(numerator: u32, denominator: u32) -> Self {
        Ratio {
            numerator,
            denominator,
        }
    }

    pub(crate) fn apply(self, side: u32) -> u32 {
        let numerator = u64::from(self.numerator);
        let denominator = u64::from(self.denominator);
        let result = (u64::from(side) * numerator + denominator / 2) / denominator;
        u32::try_from(result).unwrap_or(side).max(1)
    }

    fn is_smaller_that(self, other: Ratio) -> bool {
        u64::from(self.numerator) * u64::from(other.denominator)
            < u64::from(other.numerator) * u64::from(self.denominator)
    }
}

impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tenths = (u64::from(self.numerator) * 1000 + u64::from(self.denominator) / 2)
            / u64::from(self.denominator);
        if tenths % 10 == 0 {
            write!(f, "{} %", tenths / 10)
        } else {
            write!(f, "{},{} %", tenths / 10, tenths % 10)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Knob {
    Quality,
    Colors,
}

impl fmt::Display for Knob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Knob::Quality => "calidad",
            Knob::Colors => "colores",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KnobRange {
    pub(crate) height: u16,
    pub(crate) low: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lever {
    No,
    Fps(&'static [Fps]),
    Detail(&'static [Ratio]),
    Size,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) lever: Lever,
    pub(crate) range: KnobRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ladder {
    pub(crate) knob: Knob,
    pub(crate) requested_knob: u16,
    pub(crate) squeeze: bool,
    pub(crate) steps: &'static [Step],
}

const WEBP_QUALITY: KnobRange = KnobRange {
    height: 90,
    low: 50,
};

pub(crate) static WEBP_LADDER: Ladder = Ladder {
    knob: Knob::Quality,
    requested_knob: WEBP_QUALITY.height,
    squeeze: false,
    steps: &[
        Step {
            lever: Lever::No,
            range: WEBP_QUALITY,
        },
        Step {
            lever: Lever::Fps(&[Fps::constant::<15>(), Fps::constant::<12>()]),
            range: WEBP_QUALITY,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(3, 4), Ratio::new(1, 2)]),
            range: WEBP_QUALITY,
        },
        Step {
            lever: Lever::Fps(&[Fps::constant::<10>(), Fps::constant::<8>()]),
            range: WEBP_QUALITY,
        },
        Step {
            lever: Lever::Size,
            range: WEBP_QUALITY,
        },
    ],
};

const SHARPNESS_WEBP_QUALITY: KnobRange = KnobRange {
    height: 90,
    low: 75,
};

pub(crate) static SHARPNESS_WEBP_LADDER: Ladder = Ladder {
    knob: Knob::Quality,
    requested_knob: SHARPNESS_WEBP_QUALITY.height,
    squeeze: false,
    steps: &[
        Step {
            lever: Lever::No,
            range: SHARPNESS_WEBP_QUALITY,
        },
        Step {
            lever: Lever::Fps(&[Fps::constant::<10>()]),
            range: SHARPNESS_WEBP_QUALITY,
        },
    ],
};

const SMOOTHNESS_WEBP_QUALITY: KnobRange = KnobRange {
    height: 85,
    low: 35,
};

pub(crate) static SMOOTHNESS_WEBP_LADDER: Ladder = Ladder {
    knob: Knob::Quality,
    requested_knob: SMOOTHNESS_WEBP_QUALITY.height,
    squeeze: false,
    steps: &[
        Step {
            lever: Lever::No,
            range: SMOOTHNESS_WEBP_QUALITY,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(3, 4), Ratio::new(1, 2)]),
            range: SMOOTHNESS_WEBP_QUALITY,
        },
        Step {
            lever: Lever::Fps(&[
                Fps::constant::<15>(),
                Fps::constant::<12>(),
                Fps::constant::<10>(),
                Fps::constant::<8>(),
            ]),
            range: SMOOTHNESS_WEBP_QUALITY,
        },
        Step {
            lever: Lever::Size,
            range: SMOOTHNESS_WEBP_QUALITY,
        },
    ],
};

const ACCEPTABLE_COLORS: KnobRange = KnobRange {
    height: 256,
    low: 64,
};
const FORCED_COLORS: KnobRange = KnobRange {
    height: 63,
    low: 16,
};

pub(crate) static PALETTE_LADDER: Ladder = Ladder {
    knob: Knob::Colors,
    requested_knob: ACCEPTABLE_COLORS.height,
    squeeze: false,
    steps: &[
        Step {
            lever: Lever::No,
            range: ACCEPTABLE_COLORS,
        },
        Step {
            lever: Lever::Fps(&[
                Fps::constant::<15>(),
                Fps::constant::<12>(),
                Fps::constant::<10>(),
                Fps::constant::<8>(),
            ]),
            range: ACCEPTABLE_COLORS,
        },
        Step {
            lever: Lever::Size,
            range: ACCEPTABLE_COLORS,
        },
        Step {
            lever: Lever::No,
            range: FORCED_COLORS,
        },
    ],
};

const SHARPNESS_COLORS: KnobRange = KnobRange {
    height: 256,
    low: 128,
};

pub(crate) static SHARPNESS_PALETTE_LADDER: Ladder = Ladder {
    knob: Knob::Colors,
    requested_knob: SHARPNESS_COLORS.height,
    squeeze: false,
    steps: &[
        Step {
            lever: Lever::No,
            range: SHARPNESS_COLORS,
        },
        Step {
            lever: Lever::Fps(&[Fps::constant::<10>()]),
            range: SHARPNESS_COLORS,
        },
    ],
};

const SMOOTHNESS_COLORS: KnobRange = KnobRange {
    height: 256,
    low: 32,
};
const FORCED_SMOOTHNESS_COLORS: KnobRange = KnobRange {
    height: 31,
    low: 16,
};

pub(crate) static SMOOTHNESS_PALETTE_LADDER: Ladder = Ladder {
    knob: Knob::Colors,
    requested_knob: SMOOTHNESS_COLORS.height,
    squeeze: false,
    steps: &[
        Step {
            lever: Lever::No,
            range: SMOOTHNESS_COLORS,
        },
        Step {
            lever: Lever::Size,
            range: SMOOTHNESS_COLORS,
        },
        Step {
            lever: Lever::Fps(&[
                Fps::constant::<15>(),
                Fps::constant::<12>(),
                Fps::constant::<10>(),
                Fps::constant::<8>(),
            ]),
            range: SMOOTHNESS_COLORS,
        },
        Step {
            lever: Lever::No,
            range: FORCED_SMOOTHNESS_COLORS,
        },
    ],
};

const FIT_ANYWAY_QUALITY: KnobRange = KnobRange {
    height: 85,
    low: 35,
};
const FIT_ANYWAY_QUALITY_DEEP: KnobRange = KnobRange { height: 85, low: 5 };
const FIT_ANYWAY_QUALITY_FLOOR: KnobRange = KnobRange { height: 85, low: 0 };

pub(crate) static FIT_ANYWAY_WEBP_LADDER: Ladder = Ladder {
    knob: Knob::Quality,
    requested_knob: FIT_ANYWAY_QUALITY.height,
    squeeze: true,
    steps: &[
        Step {
            lever: Lever::No,
            range: FIT_ANYWAY_QUALITY,
        },
        Step {
            lever: Lever::Fps(&[
                Fps::constant::<15>(),
                Fps::constant::<12>(),
                Fps::constant::<10>(),
            ]),
            range: FIT_ANYWAY_QUALITY,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(3, 4)]),
            range: FIT_ANYWAY_QUALITY_DEEP,
        },
        Step {
            lever: Lever::Fps(&[
                Fps::constant::<8>(),
                Fps::constant::<6>(),
                Fps::constant::<5>(),
            ]),
            range: FIT_ANYWAY_QUALITY_DEEP,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(5, 8), Ratio::new(1, 2)]),
            range: FIT_ANYWAY_QUALITY_DEEP,
        },
        Step {
            lever: Lever::Size,
            range: FIT_ANYWAY_QUALITY_FLOOR,
        },
    ],
};

const FIT_ANYWAY_COLORS: KnobRange = KnobRange {
    height: 256,
    low: 32,
};
const FIT_ANYWAY_COLORS_FLOOR: KnobRange = KnobRange { height: 31, low: 8 };

pub(crate) static FIT_ANYWAY_PALETTE_LADDER: Ladder = Ladder {
    knob: Knob::Colors,
    requested_knob: FIT_ANYWAY_COLORS.height,
    squeeze: true,
    steps: &[
        Step {
            lever: Lever::No,
            range: FIT_ANYWAY_COLORS,
        },
        Step {
            lever: Lever::Fps(&[
                Fps::constant::<15>(),
                Fps::constant::<12>(),
                Fps::constant::<10>(),
                Fps::constant::<8>(),
                Fps::constant::<6>(),
                Fps::constant::<5>(),
            ]),
            range: FIT_ANYWAY_COLORS,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(3, 4), Ratio::new(1, 2)]),
            range: FIT_ANYWAY_COLORS,
        },
        Step {
            lever: Lever::Size,
            range: FIT_ANYWAY_COLORS,
        },
        Step {
            lever: Lever::No,
            range: FIT_ANYWAY_COLORS_FLOOR,
        },
    ],
};

const FLUID_QUALITY: KnobRange = KnobRange { height: 85, low: 5 };
const FLUID_QUALITY_UNTOUCHED: KnobRange = KnobRange {
    height: 85,
    low: 50,
};
const FLUID_QUALITY_FLOOR: KnobRange = KnobRange { height: 85, low: 0 };

// Never lowers the fps: quality first (untouched down to 50, then denoised), then the internal detail (384, 320 and 256 px on a 512 px
// canvas), and only then the size.
pub(crate) static FLUID_WEBP_LADDER: Ladder = Ladder {
    knob: Knob::Quality,
    requested_knob: FLUID_QUALITY.height,
    squeeze: true,
    steps: &[
        Step {
            lever: Lever::No,
            range: FLUID_QUALITY_UNTOUCHED,
        },
        // Same picture, now denoised: grain costs more bytes than it is worth below that quality.
        Step {
            lever: Lever::No,
            range: FLUID_QUALITY,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(3, 4), Ratio::new(5, 8), Ratio::new(1, 2)]),
            range: FLUID_QUALITY,
        },
        Step {
            lever: Lever::Size,
            range: FLUID_QUALITY_FLOOR,
        },
    ],
};

pub(crate) static FLUID_PALETTE_LADDER: Ladder = Ladder {
    knob: Knob::Colors,
    requested_knob: FIT_ANYWAY_COLORS.height,
    squeeze: true,
    steps: &[
        Step {
            lever: Lever::No,
            range: FIT_ANYWAY_COLORS,
        },
        Step {
            lever: Lever::Detail(&[Ratio::new(3, 4), Ratio::new(5, 8), Ratio::new(1, 2)]),
            range: FIT_ANYWAY_COLORS,
        },
        Step {
            lever: Lever::Size,
            range: FIT_ANYWAY_COLORS,
        },
        Step {
            lever: Lever::No,
            range: FIT_ANYWAY_COLORS_FLOOR,
        },
    ],
};

fn size_steps(canvas: Canvas) -> &'static [Ratio] {
    const EXACT_CONTENT: &[Ratio] = &[Ratio::new(7, 8), Ratio::new(3, 4)];
    const MAX_SIDE: &[Ratio] = &[Ratio::new(5, 6), Ratio::new(2, 3), Ratio::new(1, 2)];
    match canvas {
        Canvas::Exact(_) => EXACT_CONTENT,
        Canvas::Max(_) => MAX_SIDE,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Adjustments {
    fps: Fps,
    size: Ratio,
    detail: Ratio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Config {
    adjustments: Adjustments,
    range: KnobRange,
    denoise: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    pub fps: Fps,
    pub canvas: Canvas,
    pub size: Ratio,
    pub detail: Ratio,
    pub knob: Knob,
    pub value: u16,
    pub squeeze: bool,
}

impl Attempt {
    pub(crate) fn framing(&self) -> Framing {
        let side = self.canvas.side();
        let content_side = self.size.apply(side);
        let canvas_side = match self.canvas {
            Canvas::Exact(_) => side,
            Canvas::Max(_) => content_side,
        };
        Framing::new(canvas_side, content_side, self.detail.apply(canvas_side))
    }

    /// Side in pixels at which the picture is drawn before being scaled to its final size.
    pub fn detail_side(&self) -> u32 {
        self.framing().detail_side()
    }
}

impl fmt::Display for Attempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} fps", self.fps.value())?;
        if self.size != Ratio::WHOLE {
            match self.canvas {
                Canvas::Exact(_) => write!(f, ", contenido al {}", self.size)?,
                Canvas::Max(side) => write!(f, ", {} px", self.size.apply(side))?,
            }
        }
        if self.detail != Ratio::WHOLE {
            write!(f, ", detalle al {}", self.detail)?;
        }
        write!(f, ", {} {}", self.knob, self.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reduction {
    Fps { of: u32, a: u32 },
    Detail(Ratio),
    Content(Ratio),
    Resolution { of: u32, a: u32 },
    Quality { of: u16, a: u16 },
    Colors { of: u16, a: u16 },
}

impl fmt::Display for Reduction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = Ratio::WHOLE;
        match self {
            Reduction::Fps { of, a } => write!(f, "fps {of}→{a}"),
            Reduction::Detail(a) => write!(f, "detalle {whole}→{a}"),
            Reduction::Content(a) => write!(f, "contenido {whole}→{a}"),
            Reduction::Resolution { of, a } => write!(f, "resolución {of}→{a}"),
            Reduction::Quality { of, a } => write!(f, "calidad {of}→{a}"),
            Reduction::Colors { of, a } => write!(f, "colores {of}→{a}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FittingRequest<'a> {
    pub(crate) ladder: &'a Ladder,
    pub(crate) canvas: Canvas,
    pub(crate) fps: Fps,
    pub(crate) max_size: MaxSize,
    pub(crate) speed: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fitting {
    pub(crate) bytes: Vec<u8>,
    pub(crate) attempt: Attempt,
    pub(crate) reductions: Vec<Reduction>,
    pub(crate) complete_encodings: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SizeEstimate {
    pub(crate) attempt: Attempt,
    pub(crate) size_bytes: u64,
    pub(crate) fits: bool,
    pub(crate) max_seconds_s: f64,
}

pub(crate) fn fit_size<F>(
    clip: &Clip,
    request: &FittingRequest<'_>,
    tracking: Tracking<'_>,
    encode: F,
) -> Result<Fitting, SizeFittingError>
where
    F: Fn(&ScaledClip, &Attempt) -> Result<Vec<u8>, EncodingError> + Sync,
{
    let mut engine = Engine::new(request, tracking, &encode);
    let configs = configs(request);
    let (fitting, prepared) = walk_ladder(&mut engine, clip, &configs, None)?;
    if let Some(fitting) = fitting {
        return Ok(fitting);
    }
    let last = *configs.last().ok_or(SizeFittingError::EmptyLadder)?;
    let more_light = engine.attempt(last.adjustments, last.range.low);
    match encode_lightest(&mut engine, clip, &last, prepared, &encode)? {
        Lightest::Fits(fitting) => Ok(fitting),
        Lightest::DoesNotFit { best_bytes } => Err(SizeFittingError::DoesNotFitInSize {
            limit_bytes: request.max_size.bytes(),
            best_bytes,
            best_attempt: more_light,
            suggested_seconds: engine.suggest_seconds(
                clip,
                &more_light,
                last.denoise,
                best_bytes,
            )?,
        }),
    }
}

/// Tries each configuration in order until one fits. Returns the clip prepared for the last
/// configuration reached so the caller can reuse it.
fn walk_ladder<F>(
    engine: &mut Engine<'_, F>,
    clip: &Clip,
    configs: &[Config],
    mut prepared: Option<Prepared>,
) -> Result<(Option<Fitting>, Option<Prepared>), SizeFittingError>
where
    F: Fn(&ScaledClip, &Attempt) -> Result<Vec<u8>, EncodingError> + Sync,
{
    for config in configs {
        if engine.complete_encodings >= MAX_ENCODINGS_LADDER {
            break;
        }
        engine.check_cancellation()?;
        let step_clip = step_clip(clip, prepared.take(), config);
        if let Some(fitting) = engine.try_fit(&step_clip, config)? {
            return Ok((Some(fitting), None));
        }
        prepared = Some(Prepared {
            fps: config.adjustments.fps,
            denoise: config.denoise,
            clip: step_clip,
        });
    }
    Ok((None, prepared))
}

enum Lightest {
    Fits(Fitting),
    DoesNotFit { best_bytes: u64 },
}

/// Last resort once the ladder is spent: the lightest attempt, encoded whole unless it already was.
fn encode_lightest<F>(
    engine: &mut Engine<'_, F>,
    clip: &Clip,
    last: &Config,
    prepared: Option<Prepared>,
    encode: &F,
) -> Result<Lightest, SizeFittingError>
where
    F: Fn(&ScaledClip, &Attempt) -> Result<Vec<u8>, EncodingError> + Sync,
{
    let more_light = engine.attempt(last.adjustments, last.range.low);
    if let Some((size, attempt)) = engine.last_complete
        && attempt == more_light
    {
        return Ok(Lightest::DoesNotFit { best_bytes: size });
    }
    engine.check_cancellation()?;
    engine.tracking.report(Progress::FittingSize {
        attempt: engine.complete_encodings + 1,
        max: MAX_ENCODINGS_COMPLETE,
    });
    let step_clip = step_clip(clip, prepared, last);
    let scaling = scale_frames(step_clip.side(), step_clip.frames(), more_light.framing())?;
    drop(step_clip);
    let bytes = encode(&scaling, &more_light)?;
    drop(scaling);
    engine.complete_encodings += 1;
    if bytes.len() as u64 <= engine.request.max_size.bytes() {
        return Ok(Lightest::Fits(engine.finish(bytes, more_light)));
    }
    Ok(Lightest::DoesNotFit {
        best_bytes: bytes.len() as u64,
    })
}

pub(crate) fn estimate_size<F>(
    clip: &Clip,
    request: &FittingRequest<'_>,
    tracking: Tracking<'_>,
    encode: F,
) -> Result<SizeEstimate, SizeFittingError>
where
    F: Fn(&ScaledClip, &Attempt) -> Result<Vec<u8>, EncodingError> + Sync,
{
    let mut engine = Engine::new(request, tracking, &encode);
    let configs = configs(request);
    let target = ESTIMATE_MARGIN * request.max_size.bytes() as f64;
    let output_s = clip.total_duration_ms() as f64 / 1000.0;

    let last = configs.last().ok_or(SizeFittingError::EmptyLadder)?;
    engine.check_cancellation()?;
    let more_light = engine.attempt(last.adjustments, last.range.low);
    let light_clip = step_clip(clip, None, last);
    let light_sample = sample(&light_clip, more_light.framing())?;
    let (light_size, _) = engine.estimate(&light_sample, &more_light)?;
    let max_seconds_s = output_s * target / light_size.max(1.0);
    if light_size > target {
        return Ok(SizeEstimate {
            attempt: more_light,
            size_bytes: light_size.ceil() as u64,
            fits: false,
            max_seconds_s,
        });
    }

    drop(light_sample);
    let (mut from, mut until) = (0, configs.len() - 1);
    while from < until {
        engine.check_cancellation()?;
        let media = from + (until - from) / 2;
        let config = &configs[media];
        let Config {
            adjustments, range, ..
        } = *config;
        let attempt = engine.attempt(adjustments, range.low);
        let step_clip = step_clip(clip, None, config);
        let sample = sample(&step_clip, attempt.framing())?;
        let (size, _) = engine.estimate(&sample, &attempt)?;
        if size <= target {
            until = media;
        } else {
            from = media + 1;
        }
    }

    // The sample can be off for clips whose cost changes over time, so the chosen configuration is
    // confirmed with whole encodings, exactly as `fit_size` does, and the ladder goes on if needed.
    let prepared = Prepared {
        fps: more_light.fps,
        denoise: last.denoise,
        clip: light_clip,
    };
    let (fitting, prepared) = walk_ladder(&mut engine, clip, &configs[from..], Some(prepared))?;
    if let Some(fitting) = fitting {
        return Ok(SizeEstimate {
            attempt: fitting.attempt,
            size_bytes: fitting.bytes.len() as u64,
            fits: true,
            max_seconds_s,
        });
    }
    match encode_lightest(&mut engine, clip, last, prepared, &encode)? {
        Lightest::Fits(fitting) => Ok(SizeEstimate {
            attempt: fitting.attempt,
            size_bytes: fitting.bytes.len() as u64,
            fits: true,
            max_seconds_s,
        }),
        Lightest::DoesNotFit { best_bytes } => Ok(SizeEstimate {
            attempt: more_light,
            size_bytes: best_bytes,
            fits: false,
            max_seconds_s: output_s * target / best_bytes.max(1) as f64,
        }),
    }
}

struct Prepared {
    fps: Fps,
    denoise: bool,
    clip: Clip,
}

fn step_clip(clip: &Clip, prepared: Option<Prepared>, config: &Config) -> Clip {
    let fps = config.adjustments.fps;
    match prepared {
        Some(prepared) if prepared.fps == fps && prepared.denoise == config.denoise => {
            prepared.clip
        }
        previous => {
            drop(previous);
            prepare(resample_fps(clip, fps), config.denoise)
        }
    }
}

fn prepare(clip: Clip, denoise: bool) -> Clip {
    if denoise {
        remove_near_duplicates(denoise_temporal(clip))
    } else {
        remove_near_duplicates(clip)
    }
}

fn configs(request: &FittingRequest<'_>) -> Vec<Config> {
    let mut current = Adjustments {
        fps: request.fps,
        size: Ratio::WHOLE,
        detail: Ratio::WHOLE,
    };
    let mut configs: Vec<Config> = Vec::new();
    for step in request.ladder.steps {
        let range = step.range;
        match step.lever {
            Lever::No => configs.push(Config {
                adjustments: current,
                range,
                denoise: false,
            }),
            Lever::Fps(list) => {
                for &fps in list {
                    if fps < current.fps {
                        current.fps = fps;
                        configs.push(Config {
                            adjustments: current,
                            range,
                            denoise: false,
                        });
                    }
                }
            }
            Lever::Detail(list) => {
                for &detail in list {
                    if detail.is_smaller_that(current.detail) {
                        current.detail = detail;
                        configs.push(Config {
                            adjustments: current,
                            range,
                            denoise: false,
                        });
                    }
                }
            }
            Lever::Size => {
                for &size in size_steps(request.canvas) {
                    if size.is_smaller_that(current.size) {
                        current.size = size;
                        configs.push(Config {
                            adjustments: current,
                            range,
                            denoise: false,
                        });
                    }
                }
            }
        }
    }
    if request.ladder.squeeze {
        configs
            .iter_mut()
            .skip(1)
            .for_each(|config| config.denoise = true);
    }
    configs
}

struct Engine<'a, F> {
    request: &'a FittingRequest<'a>,
    tracking: Tracking<'a>,
    encode: &'a F,
    factor_correction: f64,
    complete_encodings: u32,
    last_complete: Option<(u64, Attempt)>,
}

struct Sample {
    windows: Vec<ScaledClip>,
    ratio: f64,
    exact: bool,
}

struct Candidate {
    value: u16,
    raw_estimate: f64,
    bytes_exact: Option<Vec<u8>>,
}

impl<'a, F> Engine<'a, F>
where
    F: Fn(&ScaledClip, &Attempt) -> Result<Vec<u8>, EncodingError> + Sync,
{
    fn new(request: &'a FittingRequest<'a>, tracking: Tracking<'a>, encode: &'a F) -> Self {
        Engine {
            request,
            tracking,
            encode,
            factor_correction: 1.0,
            complete_encodings: 0,
            last_complete: None,
        }
    }

    fn attempt(&self, adjustments: Adjustments, value: u16) -> Attempt {
        Attempt {
            fps: adjustments.fps,
            canvas: self.request.canvas,
            size: adjustments.size,
            detail: adjustments.detail,
            knob: self.request.ladder.knob,
            value,
            squeeze: self.request.ladder.squeeze,
        }
    }

    fn check_cancellation(&self) -> Result<(), SizeFittingError> {
        if self.tracking.cancelled() {
            Err(SizeFittingError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn try_fit(
        &mut self,
        clip: &Clip,
        config: &Config,
    ) -> Result<Option<Fitting>, SizeFittingError> {
        let adjustments = config.adjustments;
        let framing = self.attempt(adjustments, config.range.height).framing();
        let sample = sample(clip, framing)?;
        let limit_bytes = self.request.max_size.bytes();
        let mut height = config.range.height;
        let mut here_complete = 0;
        let mut complete_scaling: Option<ScaledClip> = None;

        while here_complete < MAX_ENCODINGS_COMPLETE_BY_CONFIG
            && self.complete_encodings < MAX_ENCODINGS_LADDER
        {
            self.tracking.report(Progress::FittingSize {
                attempt: self.complete_encodings + 1,
                max: MAX_ENCODINGS_COMPLETE,
            });
            let Some(candidate) = self.find(&sample, adjustments, config.range.low, height)? else {
                return Ok(None);
            };
            let attempt = self.attempt(adjustments, candidate.value);
            let bytes = match candidate.bytes_exact {
                Some(bytes) => bytes,
                None => {
                    self.check_cancellation()?;
                    let scaling = match complete_scaling.take() {
                        Some(scaling) => scaling,
                        None => scale_frames(clip.side(), clip.frames(), framing)?,
                    };
                    let bytes = (self.encode)(&scaling, &attempt)?;
                    complete_scaling = Some(scaling);
                    bytes
                }
            };
            self.complete_encodings += 1;
            here_complete += 1;
            let size = bytes.len() as u64;
            debug!(%attempt, size, "codificación completa");
            self.last_complete = Some((size, attempt));
            if size <= limit_bytes {
                return Ok(Some(self.finish(bytes, attempt)));
            }
            if candidate.raw_estimate > 0.0 {
                self.factor_correction = size as f64 / candidate.raw_estimate;
            }
            if candidate.value == config.range.low {
                return Ok(None);
            }
            height = candidate.value - 1;
        }
        Ok(None)
    }

    fn find(
        &self,
        sample: &Sample,
        adjustments: Adjustments,
        low: u16,
        height: u16,
    ) -> Result<Option<Candidate>, SizeFittingError> {
        let target = ESTIMATE_MARGIN * self.request.max_size.bytes() as f64;
        let factor = if sample.exact {
            1.0
        } else {
            self.factor_correction
        };
        let (mut from, mut until) = (low, height);
        let mut best = None;
        let mut steps = 0;
        while from <= until && steps < SEARCH_MAX_STEPS {
            self.check_cancellation()?;
            let media = from + (until - from).div_ceil(2);
            let attempt = self.attempt(adjustments, media);
            let (raw_estimate, bytes_exact) = self.estimate(sample, &attempt)?;
            steps += 1;
            if raw_estimate * factor <= target {
                best = Some(Candidate {
                    value: media,
                    raw_estimate,
                    bytes_exact,
                });
                from = media + 1;
            } else if media == low {
                break;
            } else {
                until = media - 1;
            }
        }
        Ok(best)
    }

    fn estimate(
        &self,
        sample: &Sample,
        attempt: &Attempt,
    ) -> Result<(f64, Option<Vec<u8>>), EncodingError> {
        let mut encoded = sample
            .windows
            .par_iter()
            .map(|window| (self.encode)(window, attempt))
            .collect::<Result<Vec<_>, _>>()?;
        let size: usize = encoded.iter().map(Vec::len).sum();
        let estimate = size as f64 * sample.ratio;
        let bytes_exact = if sample.exact { encoded.pop() } else { None };
        Ok((estimate, bytes_exact))
    }

    fn suggest_seconds(
        &self,
        clip: &Clip,
        more_light: &Attempt,
        denoise: bool,
        size_bytes: u64,
    ) -> Result<Option<f64>, SizeFittingError> {
        let speed = self.request.speed;
        let fps = self.request.fps;
        let output_ms = clip.total_duration_ms();
        let original_ms = (output_ms as f64 * speed).floor();
        let target = ESTIMATE_MARGIN * self.request.max_size.bytes() as f64;
        let missing_of_frames =
            (f64::from(more_light.fps.value()) * speed / f64::from(fps.value())).max(1.0);
        let mut cap_s = media_seconds_towards_below(output_ms as f64 / 1000.0);
        if cap_s * 1000.0 >= original_ms {
            cap_s -= SUGGESTION_STEP_S;
        }
        let proportional_s =
            media_seconds_towards_below(original_ms / 1000.0 * target / size_bytes.max(1) as f64);
        let mut seconds = proportional_s.min(cap_s);
        for attempt in 1..=MAX_VERIFICATIONS_SUGGESTION {
            if seconds < SUGGESTION_STEP_S {
                break;
            }
            self.check_cancellation()?;
            self.tracking.report(Progress::SearchingDuration {
                attempt,
                max: MAX_VERIFICATIONS_SUGGESTION,
            });
            let frames = (seconds * f64::from(fps.value())).floor() as u64;
            let range = prepare(
                range_without_accelerate(clip, more_light.fps, speed, frame_mark_ms(frames, fps)),
                denoise,
            );
            let scaling = scale_frames(range.side(), range.frames(), more_light.framing())?;
            let size = (self.encode)(&scaling, more_light)?.len() as f64 * missing_of_frames;
            debug!(seconds, size, "verificación de la sugerencia de segundos");
            if size <= target {
                return Ok(Some(seconds));
            }
            seconds = media_seconds_towards_below(seconds * target / size)
                .min(seconds - SUGGESTION_STEP_S);
        }
        Ok(None)
    }

    fn finish(&self, bytes: Vec<u8>, attempt: Attempt) -> Fitting {
        if matches!(attempt.canvas, Canvas::Exact(_)) && attempt.size != Ratio::WHOLE {
            warn!(
                content = %attempt.size,
                "se encogió el contenido dentro del lienzo, con borde transparente, para que quepa"
            );
        }
        Fitting {
            bytes,
            attempt,
            reductions: reductions(self.request, &attempt),
            complete_encodings: self.complete_encodings,
        }
    }
}

fn sample(clip: &Clip, framing: Framing) -> Result<Sample, ScalingError> {
    let frames = clip.frames();
    let side = clip.side();
    let total = frames.len();
    let sample_size = FRAMES_BY_WINDOW * WINDOWS;
    if total <= sample_size {
        return Ok(Sample {
            windows: vec![scale_frames(side, frames, framing)?],
            ratio: 1.0,
            exact: true,
        });
    }
    let center = total / 2 - FRAMES_BY_WINDOW / 2;
    let windows: Vec<&[Frame]> = vec![
        &frames[..FRAMES_BY_WINDOW],
        &frames[center..center + FRAMES_BY_WINDOW],
        &frames[total - FRAMES_BY_WINDOW..],
    ];
    Ok(Sample {
        windows: windows
            .into_iter()
            .map(|window| scale_frames(side, window, framing))
            .collect::<Result<_, _>>()?,
        ratio: total as f64 / sample_size as f64,
        exact: false,
    })
}

fn reductions(request: &FittingRequest<'_>, attempt: &Attempt) -> Vec<Reduction> {
    let mut reductions = Vec::new();
    if attempt.fps != request.fps {
        reductions.push(Reduction::Fps {
            of: request.fps.value(),
            a: attempt.fps.value(),
        });
    }
    if attempt.detail != Ratio::WHOLE {
        reductions.push(Reduction::Detail(attempt.detail));
    }
    if attempt.size != Ratio::WHOLE {
        reductions.push(match request.canvas {
            Canvas::Exact(_) => Reduction::Content(attempt.size),
            Canvas::Max(side) => Reduction::Resolution {
                of: side,
                a: attempt.size.apply(side),
            },
        });
    }
    let requested = request.ladder.requested_knob;
    if attempt.value != requested {
        reductions.push(match attempt.knob {
            Knob::Quality => Reduction::Quality {
                of: requested,
                a: attempt.value,
            },
            Knob::Colors => Reduction::Colors {
                of: requested,
                a: attempt.value,
            },
        });
    }
    reductions
}

fn media_seconds_towards_below(seconds: f64) -> f64 {
    (seconds / SUGGESTION_STEP_S).floor() * SUGGESTION_STEP_S
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::error::ConversionError;
    use crate::time::frame_duration_ms;

    fn clip(side: u32, fps: u32, seconds: u64) -> Clip {
        let fps = Fps::new(fps).unwrap();
        let frames = (0..seconds * u64::from(fps.value()))
            .map(|index| Frame {
                rgba: vec![(index * 7 % 256) as u8; (side * side * 4) as usize].into(),
                duration_ms: frame_duration_ms(index, fps),
            })
            .collect();
        Clip::new(side, frames).unwrap()
    }

    fn fps(value: u32) -> Fps {
        Fps::new(value).unwrap()
    }

    fn request(
        ladder: &Ladder,
        canvas: Canvas,
        fps_requested: u32,
        limit: u64,
    ) -> FittingRequest<'_> {
        FittingRequest {
            ladder,
            canvas,
            fps: fps(fps_requested),
            max_size: MaxSize::from_bytes(limit),
            speed: 1.0,
        }
    }

    fn size_formula(clip: &ScaledClip, attempt: &Attempt) -> usize {
        let detail = attempt.framing().detail_side() as usize;
        clip.frames().len() * detail * detail * usize::from(attempt.value) / 1000
    }

    #[derive(Debug, Clone, Copy)]
    struct Call {
        attempt: Attempt,
        side: u32,
        frames: usize,
    }

    struct Log {
        calls: Mutex<Vec<Call>>,
    }

    impl Log {
        fn new() -> Self {
            Log {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn encode(
            &self,
            clip: &ScaledClip,
            attempt: &Attempt,
            size: usize,
        ) -> Result<Vec<u8>, EncodingError> {
            self.calls.lock().unwrap().push(Call {
                attempt: *attempt,
                side: clip.side(),
                frames: clip.frames().len(),
            });
            Ok(vec![0; size])
        }

        fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }

        fn configs(&self) -> Vec<(u32, Ratio, Ratio)> {
            let mut views: Vec<(u32, Ratio, Ratio)> = Vec::new();
            for call in self.calls() {
                let key = (
                    call.attempt.fps.value(),
                    call.attempt.size,
                    call.attempt.detail,
                );
                if views.last() != Some(&key) {
                    views.push(key);
                }
            }
            views
        }
    }

    const fn p(numerator: u32, denominator: u32) -> Ratio {
        Ratio::new(numerator, denominator)
    }
    const WHOLE: Ratio = Ratio::WHOLE;

    #[test]
    fn chooses_quality_more_high_that_fits() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 28_500);
        let fitting = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            Ok(vec![0; size_formula(scaling, attempt)])
        })
        .unwrap();
        assert_eq!(fitting.attempt.value, 73);
        assert_eq!(fitting.attempt.fps, fps(15));
        assert_eq!(fitting.bytes.len(), 26_910);
        assert_eq!(fitting.reductions, [Reduction::Quality { of: 90, a: 73 }]);
        assert_eq!(fitting.complete_encodings, 1);
    }

    #[test]
    fn if_fits_to_first_has_no_reductions() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 10_000_000);
        let fitting = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            Ok(vec![0; size_formula(scaling, attempt)])
        })
        .unwrap();
        assert_eq!(fitting.attempt.value, 90);
        assert!(fitting.reductions.is_empty());
        assert_eq!(fitting.complete_encodings, 1);
    }

    #[test]
    fn with_45_frames_or_less_sample_is_clip_whole() {
        let clip = clip(64, 15, 3);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 10_000_000);
        let fitting = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap();
        assert!(log.calls().iter().all(|call| call.frames == 45));
        assert_eq!(fitting.bytes.len(), 45 * 64 * 64 * 90 / 1000);
    }

    #[test]
    fn sample_are_3_windows_contiguous_of_15_frames() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 10_000_000);
        fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap();
        let sizes: Vec<usize> = log.calls().iter().map(|l| l.frames).collect();
        assert_eq!(sizes.iter().filter(|&&n| n == 15).count() % 3, 0);
        assert_eq!(sizes.iter().filter(|&&n| n == 90).count(), 1);
    }

    #[test]
    fn ladder_webp_follows_order_of_user() {
        let request = request(&WEBP_LADDER, Canvas::Exact(512), 20, 500_000);
        let order: Vec<(u32, Ratio, Ratio)> = configs(&request)
            .iter()
            .map(|c| {
                (
                    c.adjustments.fps.value(),
                    c.adjustments.size,
                    c.adjustments.detail,
                )
            })
            .collect();
        assert_eq!(
            order,
            [
                (20, WHOLE, WHOLE),
                (15, WHOLE, WHOLE),
                (12, WHOLE, WHOLE),
                (12, WHOLE, p(3, 4)),
                (12, WHOLE, p(1, 2)),
                (10, WHOLE, p(1, 2)),
                (8, WHOLE, p(1, 2)),
                (8, p(7, 8), p(1, 2)),
                (8, p(3, 4), p(1, 2)),
            ]
        );
        assert!(
            configs(&request).iter().all(|c| c.range
                == KnobRange {
                    height: 90,
                    low: 50
                }),
            "WebP no tiene rango forzado"
        );
    }

    #[test]
    fn fps_never_rise_regarding_to_requested() {
        let request = request(&WEBP_LADDER, Canvas::Exact(512), 10, 500_000);
        let fps: Vec<u32> = configs(&request)
            .iter()
            .map(|c| c.adjustments.fps.value())
            .collect();
        assert_eq!(fps, [10, 10, 10, 8, 8, 8]);
    }

    #[test]
    fn engine_walks_webp_step_to_step_without_change_canvas_exact() {
        let clip = clip(64, 20, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 20, 1);
        let error = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap_err();
        assert_eq!(
            log.configs(),
            [
                (20, WHOLE, WHOLE),
                (15, WHOLE, WHOLE),
                (12, WHOLE, WHOLE),
                (12, WHOLE, p(3, 4)),
                (12, WHOLE, p(1, 2)),
                (10, WHOLE, p(1, 2)),
                (8, WHOLE, p(1, 2)),
                (8, p(7, 8), p(1, 2)),
                (8, p(3, 4), p(1, 2)),
            ]
        );
        let calls = log.calls();
        assert!(calls.iter().all(|call| call.side == 64));
        assert!(calls.iter().all(|l| (50..=90).contains(&l.attempt.value)));
        let SizeFittingError::DoesNotFitInSize { best_attempt, .. } = error else {
            panic!("se esperaba NoCabeEnPeso: {error:?}");
        };
        assert_eq!(
            (
                best_attempt.fps,
                best_attempt.size,
                best_attempt.detail,
                best_attempt.value
            ),
            (fps(8), p(3, 4), p(1, 2), 50)
        );
    }

    #[test]
    fn framing_of_webp_shrinks_content_and_low_detail_inside_of_512() {
        let attempt = Attempt {
            fps: fps(8),
            canvas: Canvas::Exact(512),
            size: p(7, 8),
            detail: p(1, 2),
            knob: Knob::Quality,
            value: 50,
            squeeze: false,
        };
        let framing = attempt.framing();
        assert_eq!(
            (
                framing.canvas_side(),
                framing.content_side(),
                framing.detail_side()
            ),
            (512, 448, 256)
        );
    }

    #[test]
    fn ladder_gif_low_fps_then_size_and_by_last_force_colors() {
        let clip = clip(48, 15, 6);
        let log = Log::new();
        let request = request(&PALETTE_LADDER, Canvas::Max(48), 15, 1);
        fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap_err();
        assert_eq!(
            log.configs(),
            [
                (15, WHOLE, WHOLE),
                (12, WHOLE, WHOLE),
                (10, WHOLE, WHOLE),
                (8, WHOLE, WHOLE),
                (8, p(5, 6), WHOLE),
                (8, p(2, 3), WHOLE),
                (8, p(1, 2), WHOLE),
            ]
        );
        let calls = log.calls();
        let mut sides: Vec<u32> = calls.iter().map(|l| l.side).collect();
        sides.dedup();
        assert_eq!(sides, [48, 40, 32, 24]);
        let first_forced = calls
            .iter()
            .position(|l| l.attempt.value < 64)
            .expect("se llegó al peldaño de colores forzados");
        assert!(calls[..first_forced].iter().all(|l| l.attempt.value >= 64));
        assert!(calls[first_forced..].iter().all(|l| {
            (16..=63).contains(&l.attempt.value) && l.side == 24 && l.attempt.fps == fps(8)
        }));
    }

    #[test]
    fn with_canvas_exact_ladder_gif_only_shrinks_content() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&PALETTE_LADDER, Canvas::Exact(64), 15, 1);
        fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap_err();
        assert!(log.calls().iter().all(|l| l.side == 64));
        let sizes: Vec<Ratio> = log.configs().iter().map(|c| c.1).collect();
        assert_eq!(
            sizes.last(),
            Some(&p(3, 4)),
            "el contenido no baja del 75 %"
        );
    }

    #[test]
    fn reductions_reflect_that_change() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 15_000);
        let fitting = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            Ok(vec![0; size_formula(scaling, attempt)])
        })
        .unwrap();
        assert_eq!(
            fitting.reductions,
            [
                Reduction::Fps { of: 15, a: 12 },
                Reduction::Detail(p(3, 4)),
                Reduction::Quality { of: 90, a: 85 },
            ]
        );
        let texts: Vec<String> = fitting.reductions.iter().map(ToString::to_string).collect();
        assert_eq!(texts, ["fps 15→12", "detalle 100 %→75 %", "calidad 90→85"]);
    }

    #[test]
    fn reductions_of_gif_name_resolution_content_and_colors() {
        let attempt = Attempt {
            fps: fps(10),
            canvas: Canvas::Max(480),
            size: p(2, 3),
            detail: WHOLE,
            knob: Knob::Colors,
            value: 128,
            squeeze: false,
        };
        let gif_request = request(&PALETTE_LADDER, Canvas::Max(480), 15, 1);
        let texts: Vec<String> = reductions(&gif_request, &attempt)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            texts,
            ["fps 15→10", "resolución 480→320", "colores 256→128"]
        );

        let attempt = Attempt {
            canvas: Canvas::Exact(128),
            size: p(7, 8),
            ..attempt
        };
        let emoji_request = request(&PALETTE_LADDER, Canvas::Exact(128), 10, 1);
        let texts: Vec<String> = reductions(&emoji_request, &attempt)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(texts, ["contenido 100 %→87,5 %", "colores 256→128"]);
    }

    fn path(
        ladder: &Ladder,
        canvas: Canvas,
        fps_requested: u32,
    ) -> Vec<(u32, Ratio, Ratio, u16, u16)> {
        configs(&request(ladder, canvas, fps_requested, 1))
            .iter()
            .map(|c| {
                let a = c.adjustments;
                (a.fps.value(), a.size, a.detail, c.range.height, c.range.low)
            })
            .collect()
    }

    #[test]
    fn sharpness_not_low_of_75_nor_of_128_colors_nor_degrades_image() {
        assert_eq!(
            path(&SHARPNESS_WEBP_LADDER, Canvas::Exact(512), 12),
            [(12, WHOLE, WHOLE, 90, 75), (10, WHOLE, WHOLE, 90, 75)]
        );
        for canvas in [Canvas::Exact(128), Canvas::Max(480)] {
            assert_eq!(
                path(&SHARPNESS_PALETTE_LADDER, canvas, 12),
                [(12, WHOLE, WHOLE, 256, 128), (10, WHOLE, WHOLE, 256, 128)]
            );
        }
    }

    #[test]
    fn smoothness_keeps_20_fps_until_exhaust_quality_and_detail_and_low_until_8() {
        assert_eq!(
            path(&SMOOTHNESS_WEBP_LADDER, Canvas::Exact(512), 20),
            [
                (20, WHOLE, WHOLE, 85, 35),
                (20, WHOLE, p(3, 4), 85, 35),
                (20, WHOLE, p(1, 2), 85, 35),
                (15, WHOLE, p(1, 2), 85, 35),
                (12, WHOLE, p(1, 2), 85, 35),
                (10, WHOLE, p(1, 2), 85, 35),
                (8, WHOLE, p(1, 2), 85, 35),
                (8, p(7, 8), p(1, 2), 85, 35),
                (8, p(3, 4), p(1, 2), 85, 35),
            ]
        );
        assert_eq!(
            path(&SMOOTHNESS_PALETTE_LADDER, Canvas::Max(480), 20),
            [
                (20, WHOLE, WHOLE, 256, 32),
                (20, p(5, 6), WHOLE, 256, 32),
                (20, p(2, 3), WHOLE, 256, 32),
                (20, p(1, 2), WHOLE, 256, 32),
                (15, p(1, 2), WHOLE, 256, 32),
                (12, p(1, 2), WHOLE, 256, 32),
                (10, p(1, 2), WHOLE, 256, 32),
                (8, p(1, 2), WHOLE, 256, 32),
                (8, p(1, 2), WHOLE, 31, 16),
            ]
        );
        assert_eq!(
            path(&SMOOTHNESS_PALETTE_LADDER, Canvas::Exact(128), 20),
            [
                (20, WHOLE, WHOLE, 256, 32),
                (20, p(7, 8), WHOLE, 256, 32),
                (20, p(3, 4), WHOLE, 256, 32),
                (15, p(3, 4), WHOLE, 256, 32),
                (12, p(3, 4), WHOLE, 256, 32),
                (10, p(3, 4), WHOLE, 256, 32),
                (8, p(3, 4), WHOLE, 256, 32),
                (8, p(3, 4), WHOLE, 31, 16),
            ]
        );
    }

    #[test]
    fn fit_anyway_goes_down_to_5_fps_quality_0_and_half_detail() {
        let webp = path(&FIT_ANYWAY_WEBP_LADDER, Canvas::Exact(512), 15);
        let last = webp.last().expect("hay pasos");
        assert_eq!(*last, (5, p(3, 4), p(1, 2), 85, 0));
        assert!(webp.iter().any(|step| step.2 == p(5, 8)));
        assert!(FIT_ANYWAY_WEBP_LADDER.squeeze);

        let palette = path(&FIT_ANYWAY_PALETTE_LADDER, Canvas::Max(480), 15);
        assert_eq!(palette.last(), Some(&(5, p(1, 2), p(1, 2), 31, 8)));
    }

    #[test]
    fn fluid_never_lowers_fps_and_goes_through_detail_before_size() {
        for (ladder, canvas) in [
            (&FLUID_WEBP_LADDER, Canvas::Exact(512)),
            (&FLUID_PALETTE_LADDER, Canvas::Max(480)),
        ] {
            let configs = configs(&request(ladder, canvas, 24, 1));
            assert!(configs.iter().all(|c| c.adjustments.fps == fps(24)));
            let details: Vec<_> = configs.iter().map(|c| c.adjustments.detail).collect();
            assert!(details.contains(&Ratio::new(1, 2)));
        }
    }

    #[test]
    fn denoise_only_after_the_first_step_of_the_squeezing_ladders() {
        let flags = |ladder, canvas| -> Vec<bool> {
            configs(&request(ladder, canvas, 24, 1))
                .iter()
                .map(|config| config.denoise)
                .collect()
        };
        for (ladder, canvas) in [
            (&FLUID_WEBP_LADDER, Canvas::Exact(512)),
            (&FIT_ANYWAY_WEBP_LADDER, Canvas::Exact(512)),
            (&FLUID_PALETTE_LADDER, Canvas::Max(480)),
        ] {
            let flags = flags(ladder, canvas);
            assert!(!flags[0] && flags[1..].iter().all(|&denoise| denoise));
        }
        for (ladder, canvas) in [
            (&WEBP_LADDER, Canvas::Exact(512)),
            (&SMOOTHNESS_WEBP_LADDER, Canvas::Exact(512)),
            (&PALETTE_LADDER, Canvas::Max(480)),
        ] {
            assert!(flags(ladder, canvas).iter().all(|&denoise| !denoise));
        }
    }

    #[test]
    fn engine_respects_min_of_each_priority() {
        for (ladder, fps_requested, min) in [
            (&SHARPNESS_WEBP_LADDER, 12, 75),
            (&SHARPNESS_PALETTE_LADDER, 12, 128),
            (&SMOOTHNESS_WEBP_LADDER, 20, 35),
            (&SMOOTHNESS_PALETTE_LADDER, 20, 16),
            (&FIT_ANYWAY_PALETTE_LADDER, 15, 8),
        ] {
            let clip = clip(64, fps_requested, 6);
            let log = Log::new();
            let error = fit_size(
                &clip,
                &request(ladder, Canvas::Exact(64), fps_requested, 1),
                Tracking::none(),
                |scaling, attempt| log.encode(scaling, attempt, size_formula(scaling, attempt)),
            )
            .unwrap_err();
            let SizeFittingError::DoesNotFitInSize { best_attempt, .. } = error else {
                panic!("se esperaba NoCabeEnPeso: {error:?}");
            };
            assert_eq!(best_attempt.value, min);
            let values: Vec<u16> = log.calls().iter().map(|l| l.attempt.value).collect();
            assert_eq!(values.iter().min(), Some(&min), "{ladder:?}");
        }
    }

    #[test]
    fn sharpness_fails_before_that_balance_and_says_how_many_seconds_fit() {
        let fit = |ladder, fps_requested| {
            fit_size(
                &clip(64, fps_requested, 6),
                &request(ladder, Canvas::Exact(64), fps_requested, 15_000),
                Tracking::none(),
                |scaling, attempt| Ok(vec![0; size_formula(scaling, attempt)]),
            )
        };
        assert!(fit(&WEBP_LADDER, 15).is_ok());
        let error = fit(&SHARPNESS_WEBP_LADDER, 12).unwrap_err();
        assert_eq!(not_fits(&error), (18_432, Some(4.5)));
    }

    #[test]
    fn smoothness_low_quality_before_that_detail_and_fps() {
        let fitting = fit_size(
            &clip(64, 20, 6),
            &request(&SMOOTHNESS_WEBP_LADDER, Canvas::Exact(64), 20, 20_000),
            Tracking::none(),
            |scaling, attempt| Ok(vec![0; size_formula(scaling, attempt)]),
        )
        .unwrap();
        assert_eq!(
            (
                fitting.attempt.fps,
                fitting.attempt.detail,
                fitting.attempt.value
            ),
            (fps(20), WHOLE, 38)
        );
        assert_eq!(fitting.reductions, [Reduction::Quality { of: 85, a: 38 }]);
    }

    #[test]
    fn when_the_estimate_does_not_fit_fewer_seconds_than_the_clip_are_reported() {
        for (ladder, canvas, fps_requested) in [
            (&FLUID_WEBP_LADDER, Canvas::Exact(64), 24),
            (&FLUID_PALETTE_LADDER, Canvas::Max(64), 24),
            (&FIT_ANYWAY_WEBP_LADDER, Canvas::Exact(64), 15),
            (&SMOOTHNESS_WEBP_LADDER, Canvas::Exact(64), 20),
            (&SHARPNESS_PALETTE_LADDER, Canvas::Max(64), 12),
            (&WEBP_LADDER, Canvas::Exact(64), 15),
            (&PALETTE_LADDER, Canvas::Max(64), 15),
        ] {
            let clip = clip(64, fps_requested, 10);
            let estimate = estimate_size(
                &clip,
                &request(ladder, canvas, fps_requested, 10),
                Tracking::none(),
                // Never zero, so even the lightest attempt is too heavy for the limit.
                |scaling, attempt| Ok(vec![0; size_formula(scaling, attempt) + 1_000]),
            )
            .unwrap();
            assert!(!estimate.fits, "{estimate:?}");
            assert!(estimate.max_seconds_s < 10.0, "{estimate:?}");
            if ladder
                .steps
                .iter()
                .all(|step| !matches!(step.lever, Lever::Fps(_)))
            {
                assert_eq!(estimate.attempt.fps, fps(fps_requested));
            }
        }
    }

    #[test]
    fn estimate_chooses_same_that_ladder_and_confirms_it_with_a_whole_encoding() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 28_500);
        let estimate = estimate_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap();
        assert!(estimate.fits);
        assert_eq!(estimate.attempt.value, 73);
        assert_eq!(estimate.attempt.fps, fps(15));
        assert_eq!(estimate.size_bytes, 26_910);
        let whole = log
            .calls()
            .iter()
            .filter(|call| call.frames > FRAMES_BY_WINDOW)
            .count();
        assert_eq!(whole, 1);
        assert!((estimate.max_seconds_s - 6.0 * 27_075.0 / 2457.6).abs() < 1e-9);
    }

    #[test]
    fn estimate_that_does_not_fit_returns_more_light_and_seconds_proportional() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 1000);
        let estimate = estimate_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap();
        assert!(!estimate.fits);
        assert_eq!(
            estimate.attempt.to_string(),
            "8 fps, contenido al 75 %, detalle al 50 %, calidad 50"
        );
        assert_eq!(estimate.size_bytes, 2458);
        assert!((estimate.max_seconds_s - 6.0 * 950.0 / 2457.6).abs() < 1e-9);
        let calls = log.calls();
        assert_eq!(calls.len(), 3, "{calls:?}");
        assert!(calls.iter().all(|call| call.attempt == estimate.attempt));
    }

    #[test]
    fn estimate_skips_steps_that_does_not_fit_without_change_choice() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 3_000);
        let estimate = estimate_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap();
        let fitting = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            Ok(vec![0; size_formula(scaling, attempt)])
        })
        .unwrap();
        assert!(estimate.fits);
        assert_eq!(
            Attempt {
                value: fitting.attempt.value,
                ..estimate.attempt
            },
            fitting.attempt
        );
        assert_eq!(estimate.attempt, fitting.attempt);
        assert_eq!(estimate.size_bytes, fitting.bytes.len() as u64);
        let samples = log.calls().len() / WINDOWS;
        assert!(samples <= 14, "{samples} estimaciones");
    }

    #[test]
    fn estimate_that_fits_means_the_whole_encoding_fits_even_if_the_sample_is_low() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 20_000);
        // The windows are 30 % lighter than the whole clip, as with content that varies over time.
        let encode = |scaling: &ScaledClip, attempt: &Attempt| {
            let size = size_formula(scaling, attempt);
            let size = if scaling.frames().len() > FRAMES_BY_WINDOW {
                size
            } else {
                size * 7 / 10
            };
            Ok(vec![0; size])
        };
        let estimate = estimate_size(&clip, &request, Tracking::none(), encode).unwrap();
        let fitting = fit_size(&clip, &request, Tracking::none(), encode).unwrap();
        assert!(estimate.fits, "{estimate:?}");
        assert!(estimate.size_bytes <= 20_000, "{estimate:?}");
        assert_eq!(estimate.attempt, fitting.attempt);
        assert_eq!(estimate.size_bytes, fitting.bytes.len() as u64);
    }

    #[test]
    fn estimate_cancels_to_half_of_ladder() {
        let clip = clip(64, 15, 6);
        let cancellation = std::sync::atomic::AtomicBool::new(false);
        let log = Log::new();
        let error = estimate_size(
            &clip,
            &request(&WEBP_LADDER, Canvas::Exact(64), 15, 28_500),
            Tracking::new(&|_| {}, &cancellation),
            |scaling, attempt| {
                cancellation.store(true, std::sync::atomic::Ordering::Relaxed);
                log.encode(scaling, attempt, size_formula(scaling, attempt))
            },
        )
        .unwrap_err();
        assert!(matches!(error, SizeFittingError::Cancelled), "{error:?}");
        assert_eq!(log.calls().len(), 3);
    }

    #[test]
    fn not_exceeds_10_encodings_complete() {
        let clip = clip(64, 15, 6);
        let limit = 10_000_000;
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, limit);
        let error = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            let size = if scaling.frames().len() > FRAMES_BY_WINDOW {
                limit as usize + 1
            } else {
                size_formula(scaling, attempt)
            };
            log.encode(scaling, attempt, size)
        })
        .unwrap_err();
        let complete = log
            .calls()
            .iter()
            .filter(|l| l.frames > FRAMES_BY_WINDOW)
            .count();
        assert_eq!(
            complete,
            (MAX_ENCODINGS_COMPLETE + MAX_VERIFICATIONS_SUGGESTION) as usize
        );
        assert!(matches!(
            error,
            SizeFittingError::DoesNotFitInSize { best_bytes, .. } if best_bytes == limit + 1
        ));
    }

    #[test]
    fn factor_of_correction_applies_to_estimates_next() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 28_500);
        let fitting = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            let mut size = size_formula(scaling, attempt);
            if scaling.frames().len() > FRAMES_BY_WINDOW {
                size *= 2;
            }
            log.encode(scaling, attempt, size)
        })
        .unwrap();
        assert_eq!(fitting.complete_encodings, 2);
        assert_eq!(
            (
                fitting.attempt.fps,
                fitting.attempt.detail,
                fitting.attempt.value
            ),
            (fps(12), p(3, 4), 81)
        );
        assert_eq!(fitting.bytes.len(), 26_872);
        let complete: Vec<u16> = log
            .calls()
            .iter()
            .filter(|l| l.frames > FRAMES_BY_WINDOW)
            .map(|l| l.attempt.value)
            .collect();
        assert_eq!(complete, [73, 81]);
    }

    #[test]
    fn error_brings_best_attempt_and_seconds_suggested() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 1000);
        let error = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            Ok(vec![0; size_formula(scaling, attempt)])
        })
        .unwrap_err();
        let SizeFittingError::DoesNotFitInSize {
            limit_bytes,
            best_bytes,
            best_attempt,
            suggested_seconds,
        } = &error
        else {
            panic!("se esperaba NoCabeEnPeso: {error:?}");
        };
        assert_eq!(*limit_bytes, 1000);
        assert_eq!(*best_bytes, 2457);
        assert_eq!(
            best_attempt.to_string(),
            "8 fps, contenido al 75 %, detalle al 50 %, calidad 50"
        );
        assert_eq!(*suggested_seconds, Some(2.0));
        assert_eq!(
            ConversionError::from_fitting("whatsapp-sticker", false, None, error).to_string(),
            "no cabe en 1 KB: el intento más ligero pesó 3 KB (8 fps, contenido al 75 %, \
             detalle al 50 %, calidad 50). Prueba con --max-seconds 2 o elige otro tramo con --start."
        );
    }

    #[test]
    fn if_encoding_of_measure_fits_is_result() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 2500);
        let fitting = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            Ok(vec![0; size_formula(scaling, attempt)])
        })
        .unwrap();
        assert_eq!(fitting.bytes.len(), 2457);
        assert_eq!(
            fitting.attempt.to_string(),
            "8 fps, contenido al 75 %, detalle al 50 %, calidad 50"
        );
        assert_eq!(fitting.complete_encodings, 1);
    }

    fn size_with_cost_fixed(clip: &ScaledClip) -> usize {
        5000 + 100 * clip.frames().len()
    }

    fn verified_frames(log: &Log) -> Vec<usize> {
        let calls = log.calls();
        let measure = calls
            .iter()
            .position(|l| l.frames == 48)
            .expect("se codificó entera la configuración más ligera");
        calls[measure + 1..].iter().map(|l| l.frames).collect()
    }

    fn not_fits(error: &SizeFittingError) -> (u64, Option<f64>) {
        let SizeFittingError::DoesNotFitInSize {
            best_bytes,
            suggested_seconds,
            ..
        } = error
        else {
            panic!("se esperaba NoCabeEnPeso: {error:?}");
        };
        (*best_bytes, *suggested_seconds)
    }

    #[test]
    fn suggestion_of_seconds_verifies_and_low_until_that_fits() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 9000);
        let error = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_with_cost_fixed(scaling))
        })
        .unwrap_err();
        assert_eq!(not_fits(&error), (9800, Some(4.0)));
        assert_eq!(verified_frames(&log), [40, 36, 32]);
        let more_light = log.calls().last().unwrap().attempt;
        assert_eq!(
            more_light.to_string(),
            "8 fps, contenido al 75 %, detalle al 50 %, calidad 50"
        );
    }

    #[test]
    fn if_no_duration_fits_not_suggests_no() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 9000);
        let error = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, 4000 + size_with_cost_fixed(scaling))
        })
        .unwrap_err();
        assert_eq!(
            verified_frames(&log).len(),
            MAX_VERIFICATIONS_SUGGESTION as usize
        );
        assert_eq!(not_fits(&error), (13_800, None));
        let message =
            ConversionError::from_fitting("whatsapp-sticker", false, None, error).to_string();
        assert!(
            message
                .ends_with("No se encontró una duración que quepa; elige otro tramo con --start."),
            "{message}"
        );
        assert!(!message.contains("--max-seconds"), "{message}");
    }

    #[test]
    fn with_fit_suggestion_verifies_to_speed_normal() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = FittingRequest {
            speed: 1.5,
            ..request(&WEBP_LADDER, Canvas::Exact(64), 15, 9300)
        };
        let error = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_with_cost_fixed(scaling))
        })
        .unwrap_err();
        assert_eq!(not_fits(&error), (9800, Some(4.5)));
        assert_eq!(verified_frames(&log), [48, 40, 36]);
    }

    #[test]
    fn with_over_1_875x_verification_count_frames_repeated() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = FittingRequest {
            speed: 2.0,
            ..request(&WEBP_LADDER, Canvas::Exact(64), 15, 9700)
        };
        let error = fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
            log.encode(scaling, attempt, size_with_cost_fixed(scaling))
        })
        .unwrap_err();
        assert_eq!(not_fits(&error), (9800, Some(4.5)));
        assert_eq!(verified_frames(&log), [45, 37, 33]);
    }

    #[test]
    fn verification_of_suggestion_respects_cancellation() {
        let clip = clip(64, 15, 6);
        let log = Log::new();
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 9000);
        let cancellation = std::sync::atomic::AtomicBool::new(false);
        let progress = |stage| {
            if matches!(stage, Progress::SearchingDuration { .. }) {
                cancellation.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        };
        let error = fit_size(
            &clip,
            &request,
            Tracking::new(&progress, &cancellation),
            |scaling, attempt| log.encode(scaling, attempt, size_with_cost_fixed(scaling)),
        )
        .unwrap_err();
        assert!(matches!(error, SizeFittingError::Cancelled), "{error:?}");
        assert_eq!(verified_frames(&log), [40]);
    }

    #[test]
    fn media_seconds_round_towards_below() {
        assert_eq!(media_seconds_towards_below(8.17), 8.0);
        assert_eq!(media_seconds_towards_below(5.0), 5.0);
        assert_eq!(media_seconds_towards_below(4.99), 4.5);
        assert_eq!(media_seconds_towards_below(0.4), 0.0);
    }

    #[test]
    fn ratios_apply_and_show_in_percent() {
        assert_eq!(p(5, 6).apply(480), 400);
        assert_eq!(p(2, 3).apply(480), 320);
        assert_eq!(p(1, 2).apply(480), 240);
        assert_eq!(p(7, 8).apply(512), 448);
        assert_eq!(p(3, 4).apply(512), 384);
        assert_eq!(p(1, 2).apply(1), 1);
        assert_eq!(p(7, 8).to_string(), "87,5 %");
        assert_eq!(p(3, 4).to_string(), "75 %");
        assert_eq!(p(5, 6).to_string(), "83,3 %");
        assert_eq!(WHOLE.to_string(), "100 %");
        assert!(p(3, 4).is_smaller_that(p(7, 8)));
        assert!(!p(7, 8).is_smaller_that(p(7, 8)));
    }

    #[test]
    fn ladders_only_have_data_valid() {
        for ladder in [
            &WEBP_LADDER,
            &SHARPNESS_WEBP_LADDER,
            &SMOOTHNESS_WEBP_LADDER,
            &PALETTE_LADDER,
            &SHARPNESS_PALETTE_LADDER,
            &SMOOTHNESS_PALETTE_LADDER,
            &FIT_ANYWAY_WEBP_LADDER,
            &FIT_ANYWAY_PALETTE_LADDER,
        ] {
            assert!(!ladder.steps.is_empty());
            for step in ladder.steps {
                assert!(step.range.low <= step.range.height);
                assert!(step.range.height <= ladder.requested_knob);
                match step.lever {
                    Lever::Fps(list) => {
                        assert!(list.windows(2).all(|par| par[0] > par[1]));
                    }
                    Lever::Detail(list) => {
                        assert!(list.iter().all(|d| d.is_smaller_that(WHOLE)));
                    }
                    Lever::No | Lever::Size => {}
                }
            }
        }
        assert!(PALETTE_LADDER.steps.iter().all(|step| {
            (crate::palette::MIN_COLORS..=crate::palette::MAX_COLORS).contains(&step.range.low)
        }));
    }

    #[test]
    fn with_cancellation_active_not_encodes_nothing() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 15_000);
        let cancellation = std::sync::atomic::AtomicBool::new(true);
        let tracking = Tracking::new(&|_| {}, &cancellation);
        let log = Log::new();
        let error = fit_size(&clip, &request, tracking, |scaling, attempt| {
            log.encode(scaling, attempt, size_formula(scaling, attempt))
        })
        .unwrap_err();
        assert!(matches!(error, SizeFittingError::Cancelled), "{error:?}");
        assert!(log.calls().is_empty());
    }

    #[test]
    fn reports_of_each_encoding_complete_with_its_number() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 15_000);
        let stages = Mutex::new(Vec::new());
        let progress = |stage| stages.lock().unwrap().push(stage);
        let cancellation = std::sync::atomic::AtomicBool::new(false);
        let fitting = fit_size(
            &clip,
            &request,
            Tracking::new(&progress, &cancellation),
            |scaling, attempt| Ok(vec![0; size_formula(scaling, attempt)]),
        )
        .unwrap();
        let stages = stages.into_inner().unwrap();
        assert!(!stages.is_empty());
        let attempts: Vec<u32> = stages
            .iter()
            .map(|stage| match *stage {
                Progress::FittingSize { attempt, max } => {
                    assert_eq!(max, MAX_ENCODINGS_COMPLETE);
                    attempt
                }
                other => panic!("etapa inesperada: {other:?}"),
            })
            .collect();
        assert!(
            attempts.windows(2).all(|par| par[0] <= par[1]),
            "{attempts:?}"
        );
        assert!(attempts.iter().all(|&n| n <= fitting.complete_encodings));
    }

    #[test]
    fn result_does_not_depend_of_number_of_threads() {
        let clip = clip(64, 15, 6);
        let request = request(&WEBP_LADDER, Canvas::Exact(64), 15, 15_000);
        let fit = || {
            fit_size(&clip, &request, Tracking::none(), |scaling, attempt| {
                Ok(vec![0; size_formula(scaling, attempt)])
            })
            .unwrap()
        };
        let thread = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap()
            .install(fit);
        assert_eq!(thread, fit());
    }
}
