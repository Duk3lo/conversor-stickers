use png::{BitDepth, BlendOp, ColorType, Compression, DisposeOp, Encoder, Filter};
use rayon::prelude::*;

use crate::clip::ScaledClip;
use crate::error::EncodingError;
use crate::palette::Palette;

const DELAY_DENOMINATOR: u16 = 1000;

pub(crate) fn encode_apng(clip: &ScaledClip, colors: u16) -> Result<Vec<u8>, EncodingError> {
    let palette = Palette::quantize(clip.frames(), colors)?;
    let delays_ms = clip
        .frames()
        .iter()
        .map(|frame| {
            u16::try_from(frame.duration_ms).map_err(|_| EncodingError::AnimationTooLong {
                duration_ms: clip.total_duration_ms(),
            })
        })
        .collect::<Result<Vec<u16>, _>>()?;
    let frame_count =
        u32::try_from(delays_ms.len()).map_err(|_| EncodingError::AnimationTooLong {
            duration_ms: clip.total_duration_ms(),
        })?;
    let indices: Vec<Vec<u8>> = clip
        .frames()
        .par_iter()
        .map(|frame| palette.index(&frame.rgba))
        .collect();

    let mut apng = Vec::new();
    let mut encoder = Encoder::new(&mut apng, clip.side(), clip.side());
    encoder.set_color(ColorType::Indexed);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_palette(palette.rgb());
    if let Some(transparent) = palette.transparent_index() {
        let mut alphas = vec![u8::MAX; usize::from(transparent)];
        alphas.push(0);
        encoder.set_trns(alphas);
    }
    encoder.set_compression(Compression::Balanced);
    encoder.set_filter(Filter::NoFilter);
    encoder
        .set_animated(frame_count, 0)
        .map_err(EncodingError::Apng)?;
    encoder
        .set_blend_op(BlendOp::Source)
        .map_err(EncodingError::Apng)?;
    encoder
        .set_dispose_op(DisposeOp::None)
        .map_err(EncodingError::Apng)?;

    let mut writer = encoder.write_header().map_err(EncodingError::Apng)?;
    for (indices, &delay_ms) in indices.iter().zip(&delays_ms) {
        writer
            .set_frame_delay(delay_ms, DELAY_DENOMINATOR)
            .map_err(EncodingError::Apng)?;
        writer
            .write_image_data(indices)
            .map_err(EncodingError::Apng)?;
    }
    writer.finish().map_err(EncodingError::Apng)?;
    Ok(apng)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;
    use crate::clip::Frame;

    const SIDE: u32 = 320;

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
                            (x + step * 7) as u8,
                            y as u8,
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

    struct RereadApng {
        dimensions: (u32, u32),
        number_frames_actl: u32,
        repetitions: u32,
        delays: Vec<(u16, u16)>,
        colors: HashSet<[u8; 4]>,
        frames: Vec<Vec<u8>>,
    }

    fn reread(bytes: &[u8]) -> RereadApng {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::ALPHA);
        let mut reader = decoder.read_info().unwrap();
        let info = reader.info();
        let dimensions = (info.width, info.height);
        let control = info.animation_control.unwrap();
        let mut reread = RereadApng {
            dimensions,
            number_frames_actl: control.num_frames,
            repetitions: control.num_plays,
            delays: Vec::new(),
            colors: HashSet::new(),
            frames: Vec::new(),
        };
        for _ in 0..control.num_frames {
            let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
            let output = reader.next_frame(&mut rgba).unwrap();
            assert_eq!(output.color_type, png::ColorType::Rgba);
            let control_frame = reader.info().frame_control.unwrap();
            assert_eq!(
                (control_frame.width, control_frame.height),
                dimensions,
                "cada frame es completo"
            );
            reread
                .delays
                .push((control_frame.delay_num, control_frame.delay_den));
            for pixel in rgba.as_chunks::<4>().0.iter() {
                reread.colors.insert(*pixel);
            }
            reread.frames.push(rgba);
        }
        reread
    }

    #[test]
    fn forward_and_lap_keeps_dimensions_frames_and_delays_exact_in_ms() {
        let durations = [67, 66, 67, 1, 8, 300, 3000];
        let clip = degraded_clip(&durations);
        let reread = reread(&encode_apng(&clip, 256).unwrap());
        assert_eq!(reread.dimensions, (SIDE, SIDE));
        assert_eq!(reread.number_frames_actl, 7);
        assert_eq!(reread.repetitions, 0, "se repite sin fin");
        let expected: Vec<(u16, u16)> = durations.iter().map(|&ms| (ms as u16, 1000)).collect();
        assert_eq!(reread.delays, expected);
    }

    #[test]
    fn uses_as_much_colors_requested() {
        let clip = degraded_clip(&[67; 3]);
        for colors in [256, 64, 16, 2] {
            let reread = reread(&encode_apng(&clip, colors).unwrap());
            assert!(
                reread.colors.len() <= usize::from(colors),
                "{colors} colores pedidos, {} en el archivo",
                reread.colors.len()
            );
        }
    }

    #[test]
    fn with_64_colors_weighs_less_that_with_256() {
        let clip = degraded_clip(&[67; 4]);
        let with_256 = encode_apng(&clip, 256).unwrap().len();
        let with_64 = encode_apng(&clip, 64).unwrap().len();
        assert!(
            with_64 < with_256,
            "64 colores: {with_64} B, 256 colores: {with_256} B"
        );
    }

    #[test]
    fn padding_has_alpha_0_and_content_is_opaque() {
        let rows_padding = 40;
        let mut frames = degraded_clip(&[67; 3]).frames().to_vec();
        for frame in &mut frames {
            Arc::make_mut(&mut frame.rgba)[..(SIDE * rows_padding * 4) as usize].fill(0);
        }
        let clip = ScaledClip::new(SIDE, frames).unwrap();
        let reread = reread(&encode_apng(&clip, 64).unwrap());
        let content_start = (SIDE * rows_padding * 4) as usize;
        for rgba in &reread.frames {
            let (padding, content) = rgba.split_at(content_start);
            assert!(padding.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0));
            assert!(
                content
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[3] == 255)
            );
        }
    }

    #[test]
    fn only_frame_follows_being_animated_and_keeps_its_duration() {
        let reread = reread(&encode_apng(&degraded_clip(&[3000]), 64).unwrap());
        assert_eq!(reread.number_frames_actl, 1);
        assert_eq!(reread.delays, [(3000, 1000)]);
    }

    #[test]
    fn frame_over_65_535_ms_is_error() {
        assert!(matches!(
            encode_apng(&degraded_clip(&[67, 70_000]), 64),
            Err(EncodingError::AnimationTooLong {
                duration_ms: 70_067
            })
        ));
    }

    #[test]
    fn returns_same_bytes_with_thread_that_with_several() {
        let clip = degraded_clip(&[67; 6]);
        let with_threads = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| encode_apng(&clip, 128).unwrap())
        };
        let thread = with_threads(1);
        assert_eq!(thread, with_threads(4));
        assert_eq!(thread, encode_apng(&clip, 128).unwrap());
    }

    #[test]
    fn rejects_colors_outside_of_range() {
        let clip = degraded_clip(&[67]);
        assert!(matches!(
            encode_apng(&clip, 1),
            Err(EncodingError::ColorsOutsideOfRange { colors: 1 })
        ));
    }
}
