use std::sync::Arc;

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{PixelType, ResizeOptions, Resizer};
use rayon::prelude::*;

use crate::clip::{Frame, ScaledClip, bytes_by_frame};
use crate::error::{ClipError, ScalingError};
use crate::framing_spec::{Align, Background, FramingMode, FramingSpec, Shape, shape_coverage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Framing {
    canvas_side: u32,
    content_side: u32,
    detail_side: u32,
}

impl Framing {
    pub(crate) fn new(canvas_side: u32, content_side: u32, detail_side: u32) -> Self {
        let canvas_side = canvas_side.max(1);
        let content_side = content_side.clamp(1, canvas_side);
        let detail_side = detail_side.clamp(1, content_side);
        Framing {
            canvas_side,
            content_side,
            detail_side,
        }
    }

    pub(crate) fn canvas_side(self) -> u32 {
        self.canvas_side
    }

    pub(crate) fn content_side(self) -> u32 {
        self.content_side
    }

    pub(crate) fn detail_side(self) -> u32 {
        self.detail_side
    }
}

pub(crate) fn scale_frames(
    origin_side: u32,
    frames: &[Frame],
    framing: Framing,
) -> Result<ScaledClip, ScalingError> {
    let frames = frames
        .par_iter()
        .map_init(Resizer::new, |resizer, frame| {
            let detail = scale(resizer, &frame.rgba, origin_side, framing.detail_side)?;
            let content = scale(resizer, &detail, framing.detail_side, framing.content_side)?;
            Ok(Frame {
                rgba: fit_with_padding(&content, framing.content_side, framing.canvas_side)?,
                duration_ms: frame.duration_ms,
            })
        })
        .collect::<Result<Vec<_>, ScalingError>>()?;
    Ok(ScaledClip::new(framing.canvas_side, frames)?)
}

pub(crate) fn scale(
    resizer: &mut Resizer,
    rgba: &Arc<[u8]>,
    origin_side: u32,
    destination_side: u32,
) -> Result<Arc<[u8]>, ScalingError> {
    if origin_side == destination_side {
        return Ok(Arc::clone(rgba));
    }
    let scaling = resize(
        resizer,
        rgba,
        (origin_side, origin_side),
        (destination_side, destination_side),
        None,
    )?;
    Ok(scaling.into())
}

pub(crate) fn crop_square(
    rgba: &[u8],
    (width, height): (u32, u32),
    side: u32,
    framing: &FramingSpec,
) -> Result<Arc<[u8]>, ScalingError> {
    if side == 0 {
        return Err(ClipError::SideNotValid { side }.into());
    }
    let (x, y, crop_side) = framing.crop_rect(width, height);
    let crop = ResizeOptions::new().crop(x, y, crop_side, crop_side);
    let square = resize(
        &mut Resizer::new(),
        rgba,
        (width, height),
        (side, side),
        Some(&crop),
    )?;
    Ok(square.into())
}

/// Frames a still image into a `canvas_side` square: the crop or the fitted whole image drawn
/// on its background, masked by the shape and centred at the framing's scale on a transparent
/// canvas. It mirrors what `FramingSpec::canvas_filter` asks ffmpeg to do for videos.
pub(crate) fn frame_image(
    rgba: &[u8],
    sides: (u32, u32),
    canvas_side: u32,
    framing: &FramingSpec,
) -> Result<Arc<[u8]>, ScalingError> {
    if canvas_side == 0 {
        return Err(ClipError::SideNotValid { side: canvas_side }.into());
    }
    let content_side = framing.content_side(canvas_side);
    let mut content = match framing.mode() {
        FramingMode::Crop => crop_square(rgba, sides, content_side, framing)?.to_vec(),
        FramingMode::Fit => fit_on_background(rgba, sides, content_side, framing)?,
    };
    apply_shape(&mut content, content_side, framing.shape());
    Ok(fit_with_padding(
        &content.into(),
        content_side,
        canvas_side,
    )?)
}

fn fit_on_background(
    rgba: &[u8],
    (width, height): (u32, u32),
    side: u32,
    framing: &FramingSpec,
) -> Result<Vec<u8>, ScalingError> {
    let bytes = bytes_by_frame(side).ok_or(ClipError::SideNotValid { side })?;
    let mut canvas = match framing.background() {
        Background::Transparent => vec![0; bytes],
        Background::Color([r, g, b]) => [r, g, b, 255].repeat(bytes / 4),
        Background::Blur => blurred_cover(rgba, (width, height), side)?,
    };
    let greater = u64::from(width.max(height)).max(1);
    let a_scale = |measure: u32| {
        let scaled = (u64::from(measure) * u64::from(side) + greater / 2) / greater;
        u32::try_from(scaled).unwrap_or(side).clamp(1, side)
    };
    let (fitted_width, fitted_height) = (a_scale(width), a_scale(height));
    let fitted = resize(
        &mut Resizer::new(),
        rgba,
        (width, height),
        (fitted_width, fitted_height),
        None,
    )?;
    let left = ((side - fitted_width) / 2) as usize;
    let above = match framing.align() {
        Align::Top => 0,
        Align::Center => (side - fitted_height) / 2,
        Align::Bottom => side - fitted_height,
    } as usize;
    let side = side as usize;
    let row_width = fitted_width as usize * 4;
    for (row, origin) in fitted.chunks_exact(row_width).enumerate() {
        let start = ((above + row) * side + left) * 4;
        for (below, over) in canvas[start..start + row_width]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(origin.as_chunks::<4>().0)
        {
            blend_over(below, over);
        }
    }
    Ok(canvas)
}

/// The image scaled to cover the square and blurred by going down to a small size and back.
fn blurred_cover(rgba: &[u8], sides: (u32, u32), side: u32) -> Result<Vec<u8>, ScalingError> {
    let small = (side / 12).max(1);
    let cover = crop_square(rgba, sides, small, &FramingSpec::default())?;
    let blurred = resize(
        &mut Resizer::new(),
        &cover,
        (small, small),
        (side, side),
        None,
    )?;
    Ok(blurred)
}

/// Draws `over` on `below` (straight alpha, both RGBA pixels).
fn blend_over(below: &mut [u8], over: &[u8]) {
    let over_alpha = u32::from(over[3]);
    let below_alpha = u32::from(below[3]) * (255 - over_alpha) / 255;
    let alpha = over_alpha + below_alpha;
    if alpha == 0 {
        return;
    }
    for channel in 0..3 {
        let mixed = u32::from(over[channel]) * over_alpha + u32::from(below[channel]) * below_alpha;
        below[channel] = u8::try_from(mixed / alpha).unwrap_or(u8::MAX);
    }
    below[3] = u8::try_from(alpha).unwrap_or(u8::MAX);
}

fn apply_shape(rgba: &mut [u8], side: u32, shape: Shape) {
    if shape == Shape::Square {
        return;
    }
    for (index, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let index = index as u32;
        let coverage = shape_coverage(shape, side, index % side, index / side);
        pixel[3] = (f64::from(pixel[3]) * coverage).round() as u8;
    }
}

fn resize(
    resizer: &mut Resizer,
    rgba: &[u8],
    (width, height): (u32, u32),
    (destination_width, destination_height): (u32, u32),
    options: Option<&ResizeOptions>,
) -> Result<Vec<u8>, ScalingError> {
    let origin =
        ImageRef::new(width, height, rgba, PixelType::U8x4).map_err(ScalingError::Image)?;
    let mut destination = Image::new(destination_width, destination_height, PixelType::U8x4);
    resizer
        .resize(&origin, &mut destination, options)
        .map_err(ScalingError::Resized)?;
    Ok(destination.into_vec())
}

pub(crate) fn fit_with_padding(
    content: &Arc<[u8]>,
    content_side: u32,
    canvas_side: u32,
) -> Result<Arc<[u8]>, ClipError> {
    if content_side == canvas_side {
        return Ok(Arc::clone(content));
    }
    center(content, (content_side, content_side), canvas_side)
}

/// The image scaled to fit inside a `width`x`height` canvas, without distortion and centred on a
/// transparent background.
pub(crate) fn contain(
    rgba: &[u8],
    (image_width, image_height): (u32, u32),
    (width, height): (u32, u32),
) -> Result<Vec<u8>, ScalingError> {
    let scale = |measure: u32, image: u32, canvas: u32, other: u32| {
        // Fit by the tighter axis: `canvas / image` is the smaller ratio on that one.
        let value =
            (u64::from(measure) * u64::from(canvas) + u64::from(image) / 2) / u64::from(image);
        u32::try_from(value).unwrap_or(other).clamp(1, other)
    };
    let (fitted_width, fitted_height) = if u64::from(width) * u64::from(image_height)
        <= u64::from(height) * u64::from(image_width)
    {
        (width, scale(image_height, image_width, width, height))
    } else {
        (scale(image_width, image_height, height, width), height)
    };
    let fitted = if (fitted_width, fitted_height) == (image_width, image_height) {
        rgba.to_vec()
    } else {
        resize(
            &mut Resizer::new(),
            rgba,
            (image_width, image_height),
            (fitted_width, fitted_height),
            None,
        )?
    };
    let mut canvas = vec![0; width as usize * height as usize * 4];
    let left = ((width - fitted_width) / 2) as usize;
    let above = ((height - fitted_height) / 2) as usize;
    let row_width = fitted_width as usize * 4;
    for (row, origin) in fitted.chunks_exact(row_width).enumerate() {
        let start = ((above + row) * width as usize + left) * 4;
        canvas[start..start + row_width].copy_from_slice(origin);
    }
    Ok(canvas)
}

fn center(
    content: &[u8],
    (width, height): (u32, u32),
    canvas_side: u32,
) -> Result<Arc<[u8]>, ClipError> {
    let bytes = bytes_by_frame(canvas_side).ok_or(ClipError::SideNotValid { side: canvas_side })?;
    let mut canvas = vec![0; bytes];
    let (width, height) = (width as usize, height as usize);
    let canvas_side = canvas_side as usize;
    let left = (canvas_side - width) / 2;
    let above = (canvas_side - height) / 2;
    let row_width = width * 4;
    for (row, origin) in content.chunks_exact(row_width).enumerate() {
        let start = ((above + row) * canvas_side + left) * 4;
        canvas[start..start + row_width].copy_from_slice(origin);
    }
    Ok(canvas.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::Clip;

    fn opaque(side: u32, rgb: [u8; 3]) -> Arc<[u8]> {
        [rgb[0], rgb[1], rgb[2], 255]
            .repeat((side * side) as usize)
            .into()
    }

    fn pixel(rgba: &[u8], side: u32, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * side + x) * 4) as usize;
        rgba[start..start + 4].try_into().unwrap()
    }

    fn opaque_box(rgba: &[u8], side: u32) -> (u32, u32, u32, u32) {
        let mut bounds = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..side {
            for x in 0..side {
                if pixel(rgba, side, x, y)[3] > 0 {
                    bounds = (
                        bounds.0.min(x),
                        bounds.1.min(y),
                        bounds.2.max(x),
                        bounds.3.max(y),
                    );
                }
            }
        }
        bounds
    }

    #[test]
    fn scale_returns_size_exact() {
        let mut resizer = Resizer::new();
        for (origin, destination) in [(480, 400), (480, 240), (512, 384), (256, 512), (7, 3)] {
            let scaling = scale(
                &mut resizer,
                &opaque(origin, [10, 20, 30]),
                origin,
                destination,
            )
            .unwrap();
            assert_eq!(
                scaling.len(),
                (destination * destination * 4) as usize,
                "{origin}->{destination}"
            );
            assert_eq!(
                pixel(&scaling, destination, destination / 2, destination / 2),
                [10, 20, 30, 255]
            );
        }
    }

    #[test]
    fn padding_has_alpha_0_and_content_is_centered() {
        let content = opaque(448, [200, 100, 50]);
        let canvas = fit_with_padding(&content, 448, 512).unwrap();
        assert_eq!(canvas.len(), 512 * 512 * 4);
        assert_eq!(opaque_box(&canvas, 512), (32, 32, 479, 479));
        for (x, y) in [(0, 0), (31, 256), (480, 256), (256, 511), (511, 511)] {
            assert_eq!(pixel(&canvas, 512, x, y), [0, 0, 0, 0], "({x}, {y})");
        }
        assert_eq!(pixel(&canvas, 512, 32, 32), [200, 100, 50, 255]);
        assert_eq!(pixel(&canvas, 512, 479, 479), [200, 100, 50, 255]);
    }

    #[test]
    fn frame_that_is_not_its_side_returns_error_at_scale_time() {
        let frames = [Frame {
            rgba: vec![0; 10].into(),
            duration_ms: 100,
        }];
        assert!(matches!(
            scale_frames(8, &frames, Framing::new(4, 4, 4)),
            Err(ScalingError::Image(_))
        ));
        assert!(matches!(
            scale_frames(8, &[], Framing::new(4, 4, 4)),
            Err(ScalingError::Clip(ClipError::WithoutFrames))
        ));
    }

    #[test]
    fn fit_or_scale_without_change_side_not_backup_frame() {
        let content = opaque(16, [1, 2, 3]);
        let fitted = fit_with_padding(&content, 16, 16).unwrap();
        assert!(Arc::ptr_eq(&fitted, &content));
        let scaling = scale(&mut Resizer::new(), &content, 16, 16).unwrap();
        assert!(Arc::ptr_eq(&scaling, &content));
    }

    #[test]
    fn framing_meets_its_invariant() {
        let framing = Framing::new(512, 600, 700);
        assert_eq!(
            (
                framing.canvas_side(),
                framing.content_side(),
                framing.detail_side()
            ),
            (512, 512, 512)
        );
        let framing = Framing::new(0, 0, 0);
        assert_eq!(
            (
                framing.canvas_side(),
                framing.content_side(),
                framing.detail_side()
            ),
            (1, 1, 1)
        );
    }

    fn clip_with_mark_center(side: u32) -> Clip {
        let mut rgba = opaque(side, [0, 0, 255]).to_vec();
        let fourth = side / 4;
        for y in side / 2 - fourth / 2..side / 2 + fourth / 2 {
            for x in side / 2 - fourth / 2..side / 2 + fourth / 2 {
                let start = ((y * side + x) * 4) as usize;
                rgba[start..start + 3].copy_from_slice(&[255, 0, 0]);
            }
        }
        let frames = (0..4)
            .map(|_| Frame {
                rgba: rgba.clone().into(),
                duration_ms: 67,
            })
            .collect();
        Clip::new(side, frames).unwrap()
    }

    #[test]
    fn scale_frames_low_side_real_keeping_durations() {
        let clip = clip_with_mark_center(480);
        let scaling =
            scale_frames(clip.side(), clip.frames(), Framing::new(320, 320, 320)).unwrap();
        assert_eq!(scaling.side(), 320);
        assert_eq!(scaling.frames().len(), 4);
        assert_eq!(scaling.total_duration_ms(), clip.total_duration_ms());
        let frame = &scaling.frames()[0].rgba;
        assert_eq!(pixel(frame, 320, 160, 160), [255, 0, 0, 255]);
        assert_eq!(pixel(frame, 320, 10, 10), [0, 0, 255, 255]);
    }

    #[test]
    fn scale_frames_with_canvas_exact_shrinks_only_content() {
        let clip = clip_with_mark_center(512);
        let scaling =
            scale_frames(clip.side(), clip.frames(), Framing::new(512, 384, 384)).unwrap();
        assert_eq!(scaling.side(), 512);
        for frame in scaling.frames() {
            assert_eq!(opaque_box(&frame.rgba, 512), (64, 64, 447, 447));
            assert_eq!(pixel(&frame.rgba, 512, 256, 256), [255, 0, 0, 255]);
            assert_eq!(pixel(&frame.rgba, 512, 0, 0)[3], 0);
        }
    }

    #[test]
    fn lower_detail_keeps_size_and_loses_sharpness() {
        let side = 64;
        let rgba: Vec<u8> = (0..side * side)
            .flat_map(|i| {
                let white = (i % side + i / side) % 2 == 0;
                let value = if white { 255 } else { 0 };
                [value, value, value, 255]
            })
            .collect();
        let clip = Clip::new(
            side,
            vec![Frame {
                rgba: rgba.into(),
                duration_ms: 100,
            }],
        )
        .unwrap();

        let sharp =
            scale_frames(clip.side(), clip.frames(), Framing::new(side, side, side)).unwrap();
        assert_eq!(sharp.frames()[0].rgba, clip.frames()[0].rgba);

        let blurry = scale_frames(
            clip.side(),
            clip.frames(),
            Framing::new(side, side, side / 2),
        )
        .unwrap();
        assert_eq!(blurry.side(), side);
        let center = pixel(&blurry.frames()[0].rgba, side, 32, 32);
        assert!((100..=155).contains(&center[0]), "{center:?}");
        assert_eq!(center[3], 255);
    }

    /// 90x60 image: left half red, right half blue.
    fn halves() -> Vec<u8> {
        (0..90 * 60)
            .flat_map(|i| {
                if i % 90 < 45 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect()
    }

    fn style(background: Background, shape: Shape, scale: f64, align: Align) -> FramingSpec {
        FramingSpec::fit()
            .with_style(background, shape, scale, align)
            .unwrap()
    }

    #[test]
    fn image_fit_aligns_vertically_over_each_background() {
        // Fitted in 60 the image is 60x40: 10 transparent rows above and below when centred.
        let framed = |background, align| {
            frame_image(
                &halves(),
                (90, 60),
                60,
                &style(background, Shape::Square, 1.0, align),
            )
            .unwrap()
        };
        let top = framed(Background::Transparent, Align::Top);
        assert_eq!(pixel(&top, 60, 10, 5), [255, 0, 0, 255]);
        assert_eq!(pixel(&top, 60, 10, 55)[3], 0);
        let bottom = framed(Background::Transparent, Align::Bottom);
        assert_eq!(pixel(&bottom, 60, 10, 5)[3], 0);
        assert_eq!(pixel(&bottom, 60, 50, 55), [0, 0, 255, 255]);
        let centre = framed(Background::Transparent, Align::Center);
        assert_eq!(pixel(&centre, 60, 10, 5)[3], 0);
        assert_eq!(pixel(&centre, 60, 10, 30), [255, 0, 0, 255]);

        let green = framed(Background::Color([0, 255, 0]), Align::Center);
        assert_eq!(pixel(&green, 60, 30, 2), [0, 255, 0, 255]);
        assert_eq!(pixel(&green, 60, 10, 30), [255, 0, 0, 255]);

        let blurred = framed(Background::Blur, Align::Center);
        let (left, right) = (pixel(&blurred, 60, 5, 2), pixel(&blurred, 60, 55, 2));
        assert!(left[0] > 150 && left[2] < 100 && left[3] == 255, "{left:?}");
        assert!(
            right[2] > 150 && right[0] < 100 && right[3] == 255,
            "{right:?}"
        );
        assert_eq!(pixel(&blurred, 60, 10, 30), [255, 0, 0, 255]);
    }

    #[test]
    fn image_shapes_mask_the_corners_and_scale_leaves_a_margin() {
        for shape in [Shape::Circle, Shape::Rounded] {
            let framed = frame_image(
                &halves(),
                (90, 60),
                60,
                &style(Background::Color([0, 255, 0]), shape, 1.0, Align::Center),
            )
            .unwrap();
            assert_eq!(pixel(&framed, 60, 0, 0)[3], 0, "{shape:?}");
            assert_eq!(pixel(&framed, 60, 59, 59)[3], 0, "{shape:?}");
            assert_eq!(pixel(&framed, 60, 30, 1)[3], 255, "{shape:?}");
        }
        let small = frame_image(
            &halves(),
            (90, 60),
            60,
            &style(
                Background::Color([0, 255, 0]),
                Shape::Square,
                0.5,
                Align::Center,
            ),
        )
        .unwrap();
        assert_eq!(opaque_box(&small, 60), (15, 15, 44, 44));
    }
}
