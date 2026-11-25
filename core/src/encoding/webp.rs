use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use webp_animation::{Encoder, EncoderOptions, EncodingConfig, EncodingType, LossyEncodingConfig};

use crate::clip::ScaledClip;
use crate::error::EncodingError;

pub const MAX_WEBP_QUALITY: u8 = 100;

/// Without keyframes libwebp turns pixels that barely changed into transparent ones in the next
/// frame, and in zooms or camera moves the error piles up (about 16 dB against 26 dB). A keyframe
/// at most every 10 frames bounds it for about 1 % more bytes on such clips.
const KEYFRAME_MIN_SPACING: isize = 6;
const KEYFRAME_MAX_SPACING: isize = 10;

static LIBWEBP_STARTED: Mutex<bool> = Mutex::new(false);

fn start_libwebp() -> Result<(), EncodingError> {
    let mut started = LIBWEBP_STARTED
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if *started {
        return Ok(());
    }
    let options = EncoderOptions {
        encoding_config: Some(EncodingConfig::new_lossy(75.0)),
        ..EncoderOptions::default()
    };
    let mut encoder = Encoder::new_with_options((16, 16), options).map_err(EncodingError::WebP)?;
    encoder
        .add_frame(&[u8::MAX; 16 * 16 * 4], 0)
        .map_err(EncodingError::WebP)?;
    encoder.finalize(1).map_err(EncodingError::WebP)?;
    *started = true;
    Ok(())
}

/// With `squeeze` the encoder spends more time and degrades transparency edges to save bytes.
fn lossy_config(quality: u8, squeeze: bool) -> EncodingConfig {
    if !squeeze {
        return EncodingConfig::new_lossy(f32::from(quality));
    }
    EncodingConfig {
        encoding_type: EncodingType::Lossy(LossyEncodingConfig {
            segments: 4,
            alpha_quality: 40,
            ..LossyEncodingConfig::default()
        }),
        quality: f32::from(quality),
        method: 6,
    }
}

pub(crate) fn encode_webp(
    clip: &ScaledClip,
    quality: u8,
    squeeze: bool,
    frame_min_duration_ms: Option<u32>,
) -> Result<Vec<u8>, EncodingError> {
    encode_webp_cancelable(
        clip,
        quality,
        squeeze,
        frame_min_duration_ms,
        &AtomicBool::new(false),
    )
}

/// Looks at `cancellation` before each frame, so cancelling does not wait for the whole encoding.
pub(crate) fn encode_webp_cancelable(
    clip: &ScaledClip,
    quality: u8,
    squeeze: bool,
    frame_min_duration_ms: Option<u32>,
    cancellation: &AtomicBool,
) -> Result<Vec<u8>, EncodingError> {
    if quality > MAX_WEBP_QUALITY {
        return Err(EncodingError::QualityOutsideOfRange { quality });
    }
    start_libwebp()?;
    if let Some(min_ms) = frame_min_duration_ms {
        let short = clip
            .frames()
            .iter()
            .enumerate()
            .find(|(_, frame)| frame.duration_ms < min_ms);
        if let Some((index, frame)) = short {
            return Err(EncodingError::FrameTooShort {
                index,
                duration_ms: frame.duration_ms,
                min_ms,
            });
        }
    }
    let mark_i32 = |mark_ms: u64| {
        i32::try_from(mark_ms).map_err(|_| EncodingError::AnimationTooLong {
            duration_ms: clip.total_duration_ms(),
        })
    };

    let options = EncoderOptions {
        kmin: KEYFRAME_MIN_SPACING,
        kmax: KEYFRAME_MAX_SPACING,
        encoding_config: Some(lossy_config(quality, squeeze)),
        ..EncoderOptions::default()
    };
    let side = clip.side();
    let mut encoder =
        Encoder::new_with_options((side, side), options).map_err(EncodingError::WebP)?;
    let mut mark_ms: u64 = 0;
    for frame in clip.frames() {
        if cancellation.load(Ordering::Relaxed) {
            return Err(EncodingError::Cancelled);
        }
        encoder
            .add_frame(&frame.rgba, mark_i32(mark_ms)?)
            .map_err(EncodingError::WebP)?;
        mark_ms += u64::from(frame.duration_ms);
    }
    let webp = encoder
        .finalize(mark_i32(mark_ms)?)
        .map_err(EncodingError::WebP)?
        .to_vec();
    if is_animated(&webp) {
        Ok(webp)
    } else {
        animate_fixed_image(&webp, side, clip.total_duration_ms(), frame_min_duration_ms)
    }
}

const ANIMATION_FLAG: u8 = 0x02;
const TRANSPARENCY_FLAG: u8 = 0x10;
const ANMF_WITHOUT_BLEND: u8 = 0x02;
const ANMF_BLENDED: u8 = 0x00;
const ANIM_BACKGROUND_AND_LOOP: [u8; 6] = [0xff, 0xff, 0xff, 0xff, 0, 0];
const VP_8_L_PIXEL_TRANSPARENT: [u8; 16] = [
    b'V', b'P', b'8', b'L', 0x08, 0x00, 0x00, 0x00, 0x2f, 0x00, 0x00, 0x00, 0x10, 0x88, 0x88, 0x08,
];
const DURATION_MAX_ANMF_MS: u64 = 0xff_ffff;

/// A small still WebP of a square RGBA frame, for thumbnails.
pub fn encode_thumbnail_webp(rgba: &[u8], side: u32) -> Result<Vec<u8>, EncodingError> {
    encode_static_webp(rgba, side, THUMBNAIL_QUALITY)
}

const THUMBNAIL_QUALITY: u8 = 70;

pub(crate) fn encode_static_webp(
    rgba: &[u8],
    side: u32,
    quality: u8,
) -> Result<Vec<u8>, EncodingError> {
    if quality > MAX_WEBP_QUALITY {
        return Err(EncodingError::QualityOutsideOfRange { quality });
    }
    start_libwebp()?;
    let options = EncoderOptions {
        kmax: 1,
        encoding_config: Some(EncodingConfig::new_lossy(f32::from(quality))),
        ..EncoderOptions::default()
    };
    let mut encoder =
        Encoder::new_with_options((side, side), options).map_err(EncodingError::WebP)?;
    let mut padding = vec![0; rgba.len()];
    if let (Some(padding_alpha), Some(&alpha)) = (padding.get_mut(3), rgba.get(3)) {
        *padding_alpha = if alpha == 0 { u8::MAX } else { 0 };
    }
    encoder
        .add_frame(&padding, 0)
        .map_err(EncodingError::WebP)?;
    encoder.add_frame(rgba, 1).map_err(EncodingError::WebP)?;
    let animated = encoder.finalize(2).map_err(EncodingError::WebP)?;
    let image = second_frame(&animated, side).ok_or(EncodingError::ContainerWebPNotValid)?;
    wrap_fixed_image(image, side)
}

fn second_frame(animated: &[u8], side: u32) -> Option<&[u8]> {
    let mut rest = animated.get(12..)?;
    let mut frames = 0;
    let anmf = loop {
        let (code, content, next) = read_chunk(rest)?;
        if code == b"ANMF" {
            frames += 1;
            if frames == 2 {
                break content;
            }
        }
        rest = next;
    };
    let (header, image) = anmf.split_at_checked(16)?;
    let complete_canvas = [u24(0), u24(0), u24(side - 1), u24(side - 1)].concat();
    (header.starts_with(&complete_canvas) && !image.is_empty()).then_some(image)
}

fn wrap_fixed_image(image: &[u8], side: u32) -> Result<Vec<u8>, EncodingError> {
    let mut body = b"WEBP".to_vec();
    if image.starts_with(b"ALPH") {
        let mut vp8x = vec![TRANSPARENCY_FLAG, 0, 0, 0];
        vp8x.extend(u24(side - 1));
        vp8x.extend(u24(side - 1));
        write_chunk(&mut body, b"VP8X", &vp8x)?;
    }
    body.extend_from_slice(image);
    let mut fixed = Vec::with_capacity(body.len() + 8);
    write_chunk(&mut fixed, b"RIFF", &body)?;
    Ok(fixed)
}

pub(crate) fn read_chunk(bytes: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let (header, rest) = bytes.split_at_checked(8)?;
    let (code, size) = header.split_at(4);
    let size = usize::try_from(u32::from_le_bytes(size.try_into().ok()?)).ok()?;
    let (content, rest) = rest.split_at_checked(size)?;
    let rest = rest.get(size % 2..).unwrap_or_default();
    Some((code, content, rest))
}

pub(crate) fn read_u24(bytes: &[u8]) -> Option<u32> {
    let [a, b, c] = *bytes.first_chunk::<3>()?;
    Some(u32::from_le_bytes([a, b, c, 0]))
}

fn is_animated(webp: &[u8]) -> bool {
    webp.get(12..16) == Some(b"VP8X".as_slice())
        && webp
            .get(20)
            .is_some_and(|flags| flags & ANIMATION_FLAG != 0)
}

fn animate_fixed_image(
    webp: &[u8],
    side: u32,
    duration_ms: u64,
    frame_min_duration_ms: Option<u32>,
) -> Result<Vec<u8>, EncodingError> {
    if duration_ms > DURATION_MAX_ANMF_MS {
        return Err(EncodingError::AnimationTooLong { duration_ms });
    }
    let duration_ms = duration_ms as u32;
    let first_ms = duration_ms.div_ceil(2);
    let second_ms = duration_ms - first_ms;
    let min_ms = frame_min_duration_ms.unwrap_or(1);
    if second_ms < min_ms {
        return Err(EncodingError::FrameTooShort {
            index: 1,
            duration_ms: second_ms,
            min_ms,
        });
    }

    let mut body = b"WEBP".to_vec();
    let mut vp8x = vec![ANIMATION_FLAG | TRANSPARENCY_FLAG, 0, 0, 0];
    vp8x.extend(u24(side - 1));
    vp8x.extend(u24(side - 1));
    write_chunk(&mut body, b"VP8X", &vp8x)?;
    write_chunk(&mut body, b"ANIM", &ANIM_BACKGROUND_AND_LOOP)?;
    let image = chunks_of_image(webp)?;
    write_chunk(
        &mut body,
        b"ANMF",
        &frame_anmf(side, first_ms, ANMF_WITHOUT_BLEND, image),
    )?;
    write_chunk(
        &mut body,
        b"ANMF",
        &frame_anmf(1, second_ms, ANMF_BLENDED, &VP_8_L_PIXEL_TRANSPARENT),
    )?;

    let mut animated = Vec::with_capacity(body.len() + 8);
    write_chunk(&mut animated, b"RIFF", &body)?;
    Ok(animated)
}

fn chunks_of_image(webp: &[u8]) -> Result<&[u8], EncodingError> {
    let not_valid = || EncodingError::ContainerWebPNotValid;
    if webp.get(..4) != Some(b"RIFF".as_slice()) || webp.get(8..12) != Some(b"WEBP".as_slice()) {
        return Err(not_valid());
    }
    let mut image = webp.get(12..).ok_or_else(not_valid)?;
    if image.get(..4) == Some(b"VP8X".as_slice()) {
        image = image.get(18..).ok_or_else(not_valid)?;
    }
    if image.is_empty() {
        return Err(not_valid());
    }
    Ok(image)
}

fn frame_anmf(side: u32, duration_ms: u32, flags: u8, image: &[u8]) -> Vec<u8> {
    let mut anmf = Vec::with_capacity(16 + image.len());
    anmf.extend(u24(0));
    anmf.extend(u24(0));
    anmf.extend(u24(side - 1));
    anmf.extend(u24(side - 1));
    anmf.extend(u24(duration_ms));
    anmf.push(flags);
    anmf.extend_from_slice(image);
    anmf
}

fn write_chunk(
    destination: &mut Vec<u8>,
    code: &[u8; 4],
    content: &[u8],
) -> Result<(), EncodingError> {
    let size = u32::try_from(content.len()).map_err(|_| EncodingError::ContainerWebPNotValid)?;
    destination.extend_from_slice(code);
    destination.extend(size.to_le_bytes());
    destination.extend_from_slice(content);
    if size % 2 == 1 {
        destination.push(0);
    }
    Ok(())
}

fn u24(value: u32) -> [u8; 3] {
    let [a, b, c, _] = value.to_le_bytes();
    [a, b, c]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use webp_animation::Decoder;

    use super::*;
    use crate::clip::Frame;

    const SIDE: u32 = 64;

    fn clip_with_noise(durations_ms: &[u32]) -> ScaledClip {
        let mut seed: u32 = 0x1234_5678;
        let frames = durations_ms
            .iter()
            .map(|&duration_ms| {
                let rgba = (0..SIDE * SIDE)
                    .flat_map(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        let [r, g, b, _] = seed.to_le_bytes();
                        [r, g, b, 255]
                    })
                    .collect();
                Frame { rgba, duration_ms }
            })
            .collect();
        ScaledClip::new(SIDE, frames).unwrap()
    }

    /// Zoom-like texture: every frame differs a little from the previous one over the whole picture.
    fn clip_with_slow_zoom(frames: u32) -> ScaledClip {
        let side = 192u32;
        let frames = (0..frames)
            .map(|index| {
                let zoom = 1.0 + f64::from(index) * 0.02;
                let rgba = (0..side * side)
                    .flat_map(|pixel| {
                        let x = (f64::from(pixel % side) - 96.0) / zoom;
                        let y = (f64::from(pixel / side) - 96.0) / zoom;
                        let wave = |frequency: f64| {
                            let value = 128.0 + 127.0 * ((x * x + y * y) * frequency).sin();
                            value as u8
                        };
                        [wave(0.0025), wave(0.004), wave(0.006), 255]
                    })
                    .collect();
                Frame {
                    rgba,
                    duration_ms: 42,
                }
            })
            .collect();
        ScaledClip::new(side, frames).unwrap()
    }

    fn mean_psnr_db(clip: &ScaledClip, webp: &[u8]) -> f64 {
        let decoded: Vec<Vec<u8>> = Decoder::new(webp)
            .unwrap()
            .into_iter()
            .map(|frame| frame.data().to_vec())
            .collect();
        assert_eq!(decoded.len(), clip.frames().len());
        let total: f64 = clip
            .frames()
            .iter()
            .zip(&decoded)
            .map(|(source, decoded)| {
                let (sum, count) = source
                    .rgba
                    .chunks(4)
                    .zip(decoded.chunks(4))
                    .flat_map(|(a, b)| (0..3).map(move |c| f64::from(a[c]) - f64::from(b[c])))
                    .fold((0.0, 0.0), |(sum, count), d| (sum + d * d, count + 1.0));
                10.0 * (255.0 * 255.0 / (sum / count).max(1e-9)).log10()
            })
            .sum();
        total / decoded.len() as f64
    }

    #[test]
    fn frames_that_change_a_little_everywhere_do_not_accumulate_error() {
        let clip = clip_with_slow_zoom(60);
        for squeeze in [false, true] {
            let webp = encode_webp(&clip, 30, squeeze, Some(8)).unwrap();
            let psnr = mean_psnr_db(&clip, &webp);
            assert!(psnr >= 25.0, "squeeze {squeeze}: {psnr:.2} dB");
        }
    }

    fn reread(webp: &[u8]) -> ((u32, u32), Vec<u32>) {
        let decoder = Decoder::new(webp).unwrap();
        let dimensions = decoder.dimensions();
        let mut previous_end = 0;
        let durations = decoder
            .into_iter()
            .map(|frame| {
                let duration = frame.timestamp() - previous_end;
                previous_end = frame.timestamp();
                u32::try_from(duration).unwrap()
            })
            .collect();
        (dimensions, durations)
    }

    #[test]
    fn forward_and_lap_keeps_dimensions_frames_and_durations() {
        let durations = [67, 66, 67, 100, 8, 250];
        let clip = clip_with_noise(&durations);
        let webp = encode_webp(&clip, 80, false, Some(8)).unwrap();
        let (dimensions, reread) = reread(&webp);
        assert_eq!(dimensions, (SIDE, SIDE));
        assert_eq!(reread, durations);
    }

    #[test]
    fn only_frame_exits_animated_and_keeps_duration() {
        let clip = clip_with_noise(&[3001]);
        let webp = encode_webp(&clip, 80, false, Some(8)).unwrap();
        assert!(is_animated(&webp));
        let (dimensions, reread) = reread(&webp);
        assert_eq!(dimensions, (SIDE, SIDE));
        assert_eq!(reread, [1501, 1500]);
        let images: Vec<Vec<u8>> = Decoder::new(&webp)
            .unwrap()
            .into_iter()
            .map(|frame| frame.data().to_vec())
            .collect();
        assert_eq!(images[0], images[1]);
    }

    #[test]
    fn frames_that_libwebp_merges_in_one_exit_animated_and_keep_duration() {
        let mut clip = clip_with_noise(&[67]);
        let frame = clip.frames()[0].clone();
        clip = ScaledClip::new(SIDE, vec![frame; 3]).unwrap();
        let webp = encode_webp(&clip, 80, false, Some(8)).unwrap();
        assert!(is_animated(&webp));
        let (_, reread) = reread(&webp);
        assert_eq!(reread.iter().sum::<u32>(), 201, "{reread:?}");
    }

    #[test]
    fn only_frame_more_short_that_two_min_returns_error() {
        let clip = clip_with_noise(&[15]);
        let error = encode_webp(&clip, 80, false, Some(8)).unwrap_err();
        assert!(
            matches!(
                error,
                EncodingError::FrameTooShort {
                    index: 1,
                    duration_ms: 7,
                    min_ms: 8
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn frame_with_transparency_exits_animated() {
        let mut clip = clip_with_noise(&[1000]);
        let mut frame = clip.frames()[0].clone();
        Arc::make_mut(&mut frame.rgba)[..(SIDE * 4 * 8) as usize].fill(0);
        clip = ScaledClip::new(SIDE, vec![frame.clone()]).unwrap();
        let webp = encode_webp(&clip, 80, false, Some(8)).unwrap();
        assert!(is_animated(&webp));
        let first = Decoder::new(&webp).unwrap().into_iter().next().unwrap();
        assert!(
            first.data()[..(SIDE * 4 * 8) as usize]
                .chunks(4)
                .all(|p| p[3] == 0)
        );
    }

    fn codes(webp: &[u8]) -> Vec<Vec<u8>> {
        let mut rest = &webp[12..];
        let mut codes = Vec::new();
        while !rest.is_empty() {
            let (code, _, next) = read_chunk(rest).expect("chunks completos");
            codes.push(code.to_vec());
            rest = next;
        }
        codes
    }

    #[test]
    fn webp_static_not_carries_animation_and_takes_all_canvas() {
        let side = SIDE as usize;
        let noise = clip_with_noise(&[100]).frames()[0].rgba.to_vec();
        let mut with_border = noise.clone();
        for (index, pixel) in with_border.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (index % side, index / side);
            if !(16..48).contains(&x) || !(16..48).contains(&y) {
                pixel.fill(0);
            }
        }
        let mut only_corner = vec![0; side * side * 4];
        only_corner[..4].copy_from_slice(&[200, 10, 10, 255]);
        let cases: [(&str, Vec<u8>); 5] = [
            ("ruido opaco", noise),
            ("con borde transparente", with_border),
            ("transparente", vec![0; side * side * 4]),
            ("liso", [9, 9, 9, 255].repeat(side * side)),
            ("un solo píxel", only_corner),
        ];
        for (name, rgba) in cases {
            let webp = encode_static_webp(&rgba, SIDE, 80).unwrap();
            assert!(!is_animated(&webp), "{name}");
            for code in codes(&webp) {
                assert!(code != b"ANIM" && code != b"ANMF", "{name}");
            }
            let decoder = Decoder::new(&webp).unwrap();
            assert_eq!(decoder.dimensions(), (SIDE, SIDE), "{name}");
            let frames: Vec<_> = decoder.into_iter().collect();
            assert_eq!(frames.len(), 1, "{name}");
            for (read, original) in frames[0]
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(rgba.as_chunks::<4>().0.iter())
            {
                assert_eq!(read[3], original[3], "{name}: cambió el alfa");
            }
        }
    }

    #[test]
    fn webp_static_keeps_colors_inside_of_border_transparent() {
        let side = SIDE as usize;
        let rgba: Vec<u8> = (0..side * side)
            .flat_map(|index| {
                let (x, y) = (index % side, index / side);
                if (16..48).contains(&x) && (16..48).contains(&y) {
                    [220, 30, 40, 255]
                } else {
                    [0; 4]
                }
            })
            .collect();
        let webp = encode_static_webp(&rgba, SIDE, 90).unwrap();
        let image = Decoder::new(&webp).unwrap().into_iter().next().unwrap();
        let center = &image.data()[(32 * side + 32) * 4..][..3];
        for (read, expected) in center.iter().zip([220, 30, 40]) {
            assert!(read.abs_diff(expected) < 12, "{center:?}");
        }
    }

    #[test]
    fn webp_static_is_deterministic_and_valid_quality() {
        let rgba = clip_with_noise(&[100]).frames()[0].rgba.to_vec();
        let webp = encode_static_webp(&rgba, SIDE, 70).unwrap();
        assert_eq!(webp, encode_static_webp(&rgba, SIDE, 70).unwrap());
        assert!(encode_static_webp(&rgba, SIDE, 40).unwrap().len() < webp.len());
        assert!(matches!(
            encode_static_webp(&rgba, SIDE, 101),
            Err(EncodingError::QualityOutsideOfRange { quality: 101 })
        ));
    }

    #[test]
    fn squeeze_returns_less_size_than_the_normal_encoding() {
        let clip = clip_with_noise(&[67; 5]);
        let normal = encode_webp(&clip, 50, false, None).unwrap();
        let squeezed = encode_webp(&clip, 50, true, None).unwrap();
        assert!(
            squeezed.len() < normal.len(),
            "{} {}",
            squeezed.len(),
            normal.len()
        );
    }

    #[test]
    fn less_quality_returns_less_size() {
        let clip = clip_with_noise(&[67; 5]);
        let sizes: Vec<usize> = [90, 70, 50, 10]
            .iter()
            .map(|&quality| encode_webp(&clip, quality, false, None).unwrap().len())
            .collect();
        assert!(sizes.windows(2).all(|par| par[0] > par[1]), "{sizes:?}");
    }

    #[test]
    fn same_input_returns_same_bytes() {
        let clip = clip_with_noise(&[67; 4]);
        assert_eq!(
            encode_webp(&clip, 75, false, None).unwrap(),
            encode_webp(&clip, 75, false, None).unwrap()
        );
    }

    #[test]
    fn frame_more_short_that_min_returns_error() {
        let clip = clip_with_noise(&[67, 7, 67]);
        let error = encode_webp(&clip, 80, false, Some(8)).unwrap_err();
        assert!(
            matches!(
                error,
                EncodingError::FrameTooShort {
                    index: 1,
                    duration_ms: 7,
                    min_ms: 8
                }
            ),
            "{error:?}"
        );
        assert!(error.to_string().contains("8 ms"), "{error}");
        assert!(encode_webp(&clip, 80, false, None).is_ok());
    }

    #[test]
    fn rejects_quality_greater_that_100() {
        let clip = clip_with_noise(&[67]);
        assert!(matches!(
            encode_webp(&clip, 101, false, None),
            Err(EncodingError::QualityOutsideOfRange { quality: 101 })
        ));
    }

    #[test]
    fn cancelled_flag_stops_the_encoding_before_the_first_frame() {
        let clip = clip_with_noise(&[67, 67, 67]);
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            encode_webp_cancelable(&clip, 80, false, None, &cancelled),
            Err(EncodingError::Cancelled)
        ));
    }
}
