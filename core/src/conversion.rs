use std::sync::atomic::AtomicBool;

use tracing::info;

use crate::clip::{Clip, ScaledClip};
use crate::decoding::Decoder;
use crate::encoding::{GifEncoder, GifNeuQuant, encode_apng, encode_webp_cancelable};
use crate::error::{ConversionError, EncodingError, SizeFittingError};
use crate::options::{ConversionOptions, Fps, MaxSize};
use crate::preset::{Format, Preset};
use crate::preview::encode_preview;
use crate::size_fitting::{Attempt, FittingRequest, Reduction, estimate_size, fit_size};
use crate::time::{TimePlan, plan_time};
use crate::tracking::{Progress, Tracking};

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub preset: &'static str,
    pub bytes: Vec<u8>,
    pub max_size: MaxSize,
    pub plan: TimePlan,
    pub duration_ms: u64,
    pub fps_requested: Fps,
    pub attempt: Attempt,
    pub canvas_side: u32,
    pub content_side: u32,
    pub reductions: Vec<Reduction>,
    pub complete_encodings: u32,
    pub preview: Option<Vec<u8>>,
}

impl Report {
    pub fn size_bytes(&self) -> u64 {
        self.bytes.len() as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub estimated_size_bytes: u64,
    pub max_size: MaxSize,
    pub fits: bool,
    pub attempt: Attempt,
    pub plan: TimePlan,
    pub max_seconds_s: f64,
}

pub fn plan(
    decoder: &dyn Decoder,
    options: &ConversionOptions,
) -> Result<TimePlan, ConversionError> {
    let info = decoder.inspect()?;
    Ok(plan_time(info.duration_s, options)?)
}

pub fn convert(
    decoder: &dyn Decoder,
    options: &ConversionOptions,
    tracking: Tracking<'_>,
) -> Result<Report, ConversionError> {
    let preset = options.preset();
    let fps_requested = options.fps();
    let (plan, clip) = decode_range(decoder, options, tracking)?;

    let preview = if options.preview() {
        check_cancellation(tracking)?;
        tracking.report(Progress::Preview);
        let bytes = encode_preview(&clip)?;
        tracking
            .deliver_preview(&bytes)
            .map_err(ConversionError::PreviewNotSaved)?;
        Some(bytes)
    } else {
        None
    };

    check_cancellation(tracking)?;
    let request = fitting_request(options, &plan);
    let fitting = fit_size(&clip, &request, tracking, |scaling, attempt| {
        encode(preset, scaling, attempt, tracking.cancellation())
    })
    .map_err(|error| fitting_error(options, error))?;

    let framing = fitting.attempt.framing();
    let report = Report {
        preset: preset.name,
        bytes: fitting.bytes,
        max_size: options.max_size(),
        plan,
        duration_ms: clip.total_duration_ms(),
        fps_requested,
        attempt: fitting.attempt,
        canvas_side: framing.canvas_side(),
        content_side: framing.content_side(),
        reductions: fitting.reductions,
        complete_encodings: fitting.complete_encodings,
        preview,
    };
    if report.preview.is_some() && report.attempt.fps != fps_requested {
        info!(
            fps_preview = fps_requested.value(),
            fps_final = report.attempt.fps.value(),
            "para que quepa, el archivo final tiene menos fps que la vista previa"
        );
    }
    info!(
        preset = report.preset,
        size_bytes = report.size_bytes(),
        attempts = report.complete_encodings,
        "conversión terminada"
    );
    Ok(report)
}

pub fn estimate(
    decoder: &dyn Decoder,
    options: &ConversionOptions,
    tracking: Tracking<'_>,
) -> Result<Estimate, ConversionError> {
    let preset = options.preset();
    let (plan, clip) = decode_range(decoder, options, tracking)?;

    check_cancellation(tracking)?;
    let request = fitting_request(options, &plan);
    let estimated = estimate_size(&clip, &request, tracking, |scaling, attempt| {
        encode(preset, scaling, attempt, tracking.cancellation())
    })
    .map_err(|error| fitting_error(options, error))?;
    Ok(Estimate {
        estimated_size_bytes: estimated.size_bytes,
        max_size: options.max_size(),
        fits: estimated.fits,
        attempt: estimated.attempt,
        plan,
        max_seconds_s: estimated.max_seconds_s.min(preset.max_duration_s),
    })
}

fn decode_range(
    decoder: &dyn Decoder,
    options: &ConversionOptions,
    tracking: Tracking<'_>,
) -> Result<(TimePlan, Clip), ConversionError> {
    check_cancellation(tracking)?;
    let plan = decoder
        .inspect_cancelable(tracking.cancellation())
        .map_err(ConversionError::from)
        .and_then(|info| Ok(plan_time(info.duration_s, options)?))
        .map_err(|error| {
            if tracking.cancelled() {
                ConversionError::Cancelled
            } else {
                error
            }
        })?;

    check_cancellation(tracking)?;
    tracking.report(Progress::Decoding);
    let clip = decoder.decode(
        &plan,
        options.fps(),
        options.preset().canvas.side(),
        tracking.cancellation(),
    )?;
    Ok((plan, clip))
}

fn fitting_request(options: &ConversionOptions, plan: &TimePlan) -> FittingRequest<'static> {
    let preset = options.preset();
    FittingRequest {
        ladder: preset.format.ladder(options.priority()),
        canvas: preset.canvas,
        fps: options.fps(),
        max_size: options.max_size(),
        speed: match options.speed() {
            Some(_) => 1.0,
            None => plan.factor_speed,
        },
    }
}

fn fitting_error(options: &ConversionOptions, error: SizeFittingError) -> ConversionError {
    ConversionError::from_fitting(
        options.preset().name,
        options.automatic_fitting(),
        options.speed(),
        error,
    )
}

fn check_cancellation(tracking: Tracking<'_>) -> Result<(), ConversionError> {
    if tracking.cancelled() {
        Err(ConversionError::Cancelled)
    } else {
        Ok(())
    }
}

fn encode(
    preset: &Preset,
    clip: &ScaledClip,
    attempt: &Attempt,
    cancellation: &AtomicBool,
) -> Result<Vec<u8>, EncodingError> {
    match preset.format {
        Format::AnimatedWebp => encode_webp_cancelable(
            clip,
            u8::try_from(attempt.value).unwrap_or(u8::MAX),
            attempt.squeeze,
            preset.frame_min_duration_ms,
            cancellation,
        ),
        Format::Gif => GifNeuQuant.encode(clip, attempt.value),
        Format::Apng => encode_apng(clip, attempt.value),
    }
}
