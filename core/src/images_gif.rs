//! GIF made from still images: one image gives a static GIF, several give an animation.

use std::sync::Arc;

use gif::{DisposalMethod, Encoder, Frame as GifFrame, Repeat};
use thiserror::Error;

use crate::clip::Frame;
use crate::error::{DecodingError, EncodingError, ScalingError};
use crate::image::RgbaImage;
use crate::palette::{MAX_COLORS, Palette};
use crate::scaling::contain;

pub const MAX_GIF_IMAGES: usize = 50;
pub const MIN_IMAGE_MS: u32 = 100;
pub const MAX_IMAGE_MS: u32 = 5_000;
pub const DEFAULT_GIF_SIDE: u32 = 512;

#[derive(Debug, Error)]
pub enum ImagesGifError {
    #[error("elige al menos una imagen")]
    NoImages,

    #[error("se admiten como mucho {MAX_GIF_IMAGES} imágenes; se eligieron {received}")]
    TooManyImages { received: usize },

    #[error(
        "cada imagen tiene que durar entre {} y {} s",
        MIN_IMAGE_MS as f64 / 1000.0,
        MAX_IMAGE_MS as f64 / 1000.0
    )]
    DurationNotValid,

    #[error("el lado mayor del GIF tiene que ser de 1 a {} px", u16::MAX)]
    SideNotValid,

    #[error(transparent)]
    Decoding(#[from] DecodingError),

    #[error(transparent)]
    Scaling(#[from] ScalingError),

    #[error(transparent)]
    Encoding(#[from] EncodingError),
}

/// `load(index)` returns the image number `index`, one at a time and in order, so that only one
/// full-size image is in memory at once. The canvas is the first image's, with its longest side
/// limited to `max_side` (it never grows). The rest are fitted inside it without distortion and
/// centred; the empty space is transparent.
pub fn encode_images_gif(
    count: usize,
    mut load: impl FnMut(usize) -> Result<RgbaImage, DecodingError>,
    image_ms: u32,
    repeat: bool,
    max_side: u32,
) -> Result<Vec<u8>, ImagesGifError> {
    if count == 0 {
        return Err(ImagesGifError::NoImages);
    }
    if count > MAX_GIF_IMAGES {
        return Err(ImagesGifError::TooManyImages { received: count });
    }
    if !(MIN_IMAGE_MS..=MAX_IMAGE_MS).contains(&image_ms) {
        return Err(ImagesGifError::DurationNotValid);
    }
    if !(1..=u32::from(u16::MAX)).contains(&max_side) {
        return Err(ImagesGifError::SideNotValid);
    }

    let first = load(0)?;
    let longest = first.width().max(first.height());
    let (width, height) = if longest <= max_side {
        (first.width(), first.height())
    } else {
        let scaled = |measure: u32| {
            let value = (u64::from(measure) * u64::from(max_side) + u64::from(longest) / 2)
                / u64::from(longest);
            u32::try_from(value).unwrap_or(max_side).max(1)
        };
        (scaled(first.width()), scaled(first.height()))
    };
    let (gif_width, gif_height) = match (u16::try_from(width), u16::try_from(height)) {
        (Ok(width), Ok(height)) => (width, height),
        _ => return Err(ImagesGifError::SideNotValid),
    };

    let mut first = Some(first);
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let image = match first.take() {
            Some(image) => image,
            None => load(index)?,
        };
        let rgba = contain(
            image.rgba(),
            (image.width(), image.height()),
            (width, height),
        )?;
        frames.push(Frame {
            rgba: Arc::from(rgba),
            duration_ms: image_ms,
        });
    }

    let palette = Palette::quantize(&frames, MAX_COLORS)?;
    let delay = u16::try_from(image_ms / 10).map_err(|_| ImagesGifError::DurationNotValid)?;
    let mut encoder = Encoder::new(Vec::new(), gif_width, gif_height, palette.rgb())
        .map_err(EncodingError::Gif)?;
    if count > 1 && repeat {
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(EncodingError::Gif)?;
    }
    for frame in &frames {
        let mut gif_frame = GifFrame::from_indexed_pixels(
            gif_width,
            gif_height,
            palette.index(&frame.rgba),
            palette.transparent_index(),
        );
        gif_frame.delay = delay;
        gif_frame.dispose = DisposalMethod::Background;
        encoder
            .write_frame(&gif_frame)
            .map_err(EncodingError::Gif)?;
    }
    Ok(encoder.into_inner().map_err(EncodingError::Gif)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> RgbaImage {
        let rgba = [rgb[0], rgb[1], rgb[2], 255].repeat((width * height) as usize);
        RgbaImage::new(width, height, rgba).unwrap()
    }

    fn encode(
        images: &[RgbaImage],
        image_ms: u32,
        repeat: bool,
        side: u32,
    ) -> Result<Vec<u8>, ImagesGifError> {
        encode_images_gif(
            images.len(),
            |index| Ok(images[index].clone()),
            image_ms,
            repeat,
            side,
        )
    }

    struct Reread {
        size: (u16, u16),
        delays_cs: Vec<u16>,
        repeat: Option<gif::Repeat>,
        frames: Vec<Vec<u8>>,
    }

    fn reread(bytes: &[u8]) -> Reread {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(bytes).unwrap();
        let size = (decoder.width(), decoder.height());
        let mut delays_cs = Vec::new();
        let mut frames = Vec::new();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            delays_cs.push(frame.delay);
            frames.push(frame.buffer.to_vec());
        }
        Reread {
            size,
            delays_cs,
            repeat: Some(decoder.repeat()),
            frames,
        }
    }

    fn pixel(frame: &[u8], width: u16, x: usize, y: usize) -> [u8; 4] {
        let at = (y * usize::from(width) + x) * 4;
        [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
    }

    #[test]
    fn one_image_makes_a_still_gif_of_its_size() {
        let gif = encode(&[solid(40, 20, [255, 0, 0])], 500, true, 512).unwrap();
        let gif = reread(&gif);
        assert_eq!(gif.size, (40, 20));
        assert_eq!(gif.frames.len(), 1);
        assert!(gif.repeat != Some(gif::Repeat::Infinite));
    }

    #[test]
    fn several_images_make_an_animation_in_order_with_the_chosen_time() {
        let images = [
            solid(30, 30, [255, 0, 0]),
            solid(30, 30, [0, 255, 0]),
            solid(30, 30, [0, 0, 255]),
        ];
        let gif = reread(&encode(&images, 300, true, 512).unwrap());
        assert_eq!(gif.frames.len(), 3);
        assert_eq!(gif.delays_cs, [30, 30, 30]);
        assert_eq!(gif.repeat, Some(gif::Repeat::Infinite));
        let center = |frame: &Vec<u8>| pixel(frame, 30, 15, 15);
        assert_eq!(center(&gif.frames[0]), [255, 0, 0, 255]);
        assert_eq!(center(&gif.frames[1]), [0, 255, 0, 255]);
        assert_eq!(center(&gif.frames[2]), [0, 0, 255, 255]);
    }

    #[test]
    fn without_repeat_the_animation_plays_once() {
        let images = [solid(10, 10, [1, 2, 3]), solid(10, 10, [4, 5, 6])];
        let gif = reread(&encode(&images, 100, false, 512).unwrap());
        assert_ne!(gif.repeat, Some(gif::Repeat::Infinite));
    }

    #[test]
    fn longest_side_is_limited_without_changing_the_proportion() {
        let gif = reread(&encode(&[solid(1000, 500, [9, 9, 9])], 500, true, 512).unwrap());
        assert_eq!(gif.size, (512, 256));
        let small = reread(&encode(&[solid(100, 50, [9, 9, 9])], 500, true, 512).unwrap());
        assert_eq!(small.size, (100, 50));
    }

    #[test]
    fn other_sizes_fit_the_first_canvas_with_transparent_bars() {
        let images = [solid(40, 40, [255, 0, 0]), solid(80, 20, [0, 255, 0])];
        let gif = reread(&encode(&images, 500, true, 512).unwrap());
        assert_eq!(gif.size, (40, 40));
        let wide = &gif.frames[1];
        // 80x20 shrinks to 40x10 and is centred: bars above and below.
        assert_eq!(pixel(wide, 40, 20, 20), [0, 255, 0, 255]);
        assert_eq!(pixel(wide, 40, 20, 2)[3], 0);
        assert_eq!(pixel(wide, 40, 20, 37)[3], 0);
    }

    #[test]
    fn input_is_validated() {
        let one = [solid(10, 10, [0, 0, 0])];
        assert!(matches!(
            encode(&[], 500, true, 512),
            Err(ImagesGifError::NoImages)
        ));
        let many: Vec<RgbaImage> = (0..=MAX_GIF_IMAGES).map(|_| one[0].clone()).collect();
        assert!(matches!(
            encode(&many, 500, true, 512),
            Err(ImagesGifError::TooManyImages { received: 51 })
        ));
        for bad in [0, 99, 5_001] {
            assert!(matches!(
                encode(&one, bad, true, 512),
                Err(ImagesGifError::DurationNotValid)
            ));
        }
        assert!(matches!(
            encode(&one, 500, true, 0),
            Err(ImagesGifError::SideNotValid)
        ));
    }
}
