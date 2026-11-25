use std::io::Cursor;
use std::sync::Arc;

use fast_image_resize::Resizer;
use webp_animation::Decoder;

use crate::encoding::{read_chunk, read_u24};
use crate::error::{EncodingError, StickerError};
use crate::image::WHATSAPP_STATIC_STICKER;
use crate::preset::WHATSAPP_STICKER;
use crate::scaling::scale;

const PNG_HEADER: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

const ICON_MAX_MEMORY_BYTES: usize = 1_000_000;

pub const TRAY_ICON_SIDE: u32 = 96;

pub const TRAY_ICON_MAX_SIZE: u64 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickerKind {
    Static,
    Animated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhatsappSticker {
    pub kind: StickerKind,
    pub size_bytes: u64,
    pub duration_ms: Option<u64>,
}

pub fn inspect_whatsapp_sticker(bytes: &[u8]) -> Result<WhatsappSticker, StickerError> {
    let durations_ms = read_headers(bytes)?;
    let size_bytes = bytes.len() as u64;
    let (kind, duration_ms) = if durations_ms.len() > 1 {
        let min_ms = WHATSAPP_STICKER.frame_min_duration_ms.unwrap_or(0);
        if let Some(&duration_ms) = durations_ms.iter().find(|&&ms| ms < min_ms) {
            return Err(StickerError::FrameTooShort { duration_ms });
        }
        let total_ms: u64 = durations_ms.iter().map(|&ms| u64::from(ms)).sum();
        if total_ms as f64 > WHATSAPP_STICKER.max_duration_s * 1000.0 {
            return Err(StickerError::TooLong {
                duration_ms: total_ms,
            });
        }
        (StickerKind::Animated, Some(total_ms))
    } else {
        let max = WHATSAPP_STATIC_STICKER.max_size.bytes();
        if size_bytes > max {
            return Err(StickerError::TooHeavy {
                bytes: size_bytes,
                max,
            });
        }
        (StickerKind::Static, None)
    };

    let expected = durations_ms.len().max(1);
    let decoded = decoder(bytes)?.into_iter().take(expected).count();
    if decoded != expected {
        return Err(unreadable("tiene frames dañados"));
    }
    Ok(WhatsappSticker {
        kind,
        size_bytes,
        duration_ms,
    })
}

pub fn tray_icon(sticker: &[u8]) -> Result<Vec<u8>, StickerError> {
    read_headers(sticker)?;
    let first = decoder(sticker)?
        .into_iter()
        .next()
        .ok_or_else(|| unreadable("su primer frame está dañado"))?;
    let rgba: Arc<[u8]> = first.data().into();
    let icon = scale(
        &mut Resizer::new(),
        &rgba,
        WHATSAPP_STATIC_STICKER.side,
        TRAY_ICON_SIDE,
    )
    .map_err(|error| unreadable(format!("no se pudo reducir: {error}")))?;
    let png = encode_icon(&icon).map_err(StickerError::Encoding)?;
    if png.len() as u64 > TRAY_ICON_MAX_SIZE {
        return Err(icon_not_valid("pesa más de 50 KB"));
    }
    Ok(png)
}

pub fn inspect_tray_icon(png: &[u8]) -> Result<(), StickerError> {
    if png.len() as u64 > TRAY_ICON_MAX_SIZE {
        return Err(icon_not_valid("pesa más de 50 KB"));
    }
    let sides = png
        .strip_prefix(PNG_HEADER)
        .and_then(|ihdr| ihdr.first_chunk::<8>())
        .ok_or_else(|| icon_not_valid("no es un PNG"))?;
    let [a0, a1, a2, a3, b0, b1, b2, b3] = *sides;
    let width = u32::from_be_bytes([a0, a1, a2, a3]);
    let height = u32::from_be_bytes([b0, b1, b2, b3]);
    if (width, height) != (TRAY_ICON_SIDE, TRAY_ICON_SIDE) {
        return Err(icon_not_valid("no mide 96x96"));
    }

    let damaged = || icon_not_valid("el PNG está dañado");
    let limits = png::Limits {
        bytes: ICON_MAX_MEMORY_BYTES,
    };
    let mut reader = png::Decoder::new_with_limits(Cursor::new(png), limits)
        .read_info()
        .map_err(|_| damaged())?;
    let bytes = reader
        .output_buffer_size()
        .filter(|&bytes| bytes <= ICON_MAX_MEMORY_BYTES)
        .ok_or_else(damaged)?;
    reader
        .next_frame(&mut vec![0; bytes])
        .map_err(|_| damaged())?;
    Ok(())
}

fn read_headers(bytes: &[u8]) -> Result<Vec<u32>, StickerError> {
    let size = bytes.len() as u64;
    let max = WHATSAPP_STICKER.max_size.bytes();
    if size > max {
        return Err(StickerError::TooHeavy { bytes: size, max });
    }
    if !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WEBP".as_slice()) {
        return Err(StickerError::NotWebp);
    }
    let truncated = || unreadable("el archivo está truncado");
    let (_, body, _) = read_chunk(bytes).ok_or_else(truncated)?;
    let mut rest = body.get(4..).ok_or_else(truncated)?;

    let (code, content, _) = read_chunk(rest).ok_or_else(truncated)?;
    let (width, height) =
        canvas_sides(code, content).ok_or_else(|| unreadable("su cabecera no es válida"))?;
    let side = WHATSAPP_STATIC_STICKER.side;
    if (width, height) != (side, side) {
        return Err(StickerError::IncorrectSide { width, height });
    }

    let mut durations_ms = Vec::new();
    let mut with_anim = false;
    while !rest.is_empty() {
        let (code, content, next) = read_chunk(rest).ok_or_else(truncated)?;
        with_anim |= code == b"ANIM";
        if code == b"ANMF" {
            let duration_ms = content.get(12..).and_then(read_u24);
            durations_ms.push(duration_ms.ok_or_else(truncated)?);
        }
        rest = next;
    }
    if with_anim && durations_ms.len() < 2 {
        return Err(StickerError::AnimatedSingleFrame);
    }
    Ok(durations_ms)
}

fn canvas_sides(code: &[u8], content: &[u8]) -> Option<(u32, u32)> {
    match code {
        b"VP8X" => {
            let width = read_u24(content.get(4..)?)?;
            let height = read_u24(content.get(7..)?)?;
            Some((width + 1, height + 1))
        }
        b"VP8 " => {
            let [_, _, _, 0x9d, 0x01, 0x2a, a0, a1, b0, b1] = *content.first_chunk::<10>()? else {
                return None;
            };
            let side = |low, height| u32::from(u16::from_le_bytes([low, height]) & 0x3fff);
            Some((side(a0, a1), side(b0, b1)))
        }
        b"VP8L" => {
            let [0x2f, a, b, c, d] = *content.first_chunk::<5>()? else {
                return None;
            };
            let bits = u32::from_le_bytes([a, b, c, d]);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        _ => None,
    }
}

fn decoder(bytes: &[u8]) -> Result<Decoder<'_>, StickerError> {
    Decoder::new(bytes).map_err(|error| unreadable(format!("libwebp no pudo abrirlo ({error})")))
}

fn encode_icon(rgba: &[u8]) -> Result<Vec<u8>, EncodingError> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, TRAY_ICON_SIDE, TRAY_ICON_SIDE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(EncodingError::Png)?;
    writer.write_image_data(rgba).map_err(EncodingError::Png)?;
    writer.finish().map_err(EncodingError::Png)?;
    Ok(png)
}

fn unreadable(reason: impl Into<String>) -> StickerError {
    StickerError::Unreadable {
        reason: reason.into(),
    }
}

fn icon_not_valid(reason: &'static str) -> StickerError {
    StickerError::IconNotValid { reason }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use proptest::prelude::*;
    use webp_animation::{Encoder, EncoderOptions, EncodingConfig};

    use super::*;
    use crate::encoding::encode_static_webp;

    fn frame(width: u32, height: u32, tone: u8) -> Vec<u8> {
        [tone, 40, 90, 255].repeat((width * height) as usize)
    }

    fn animated_with(width: u32, height: u32, durations_ms: &[i32]) -> Vec<u8> {
        let options = EncoderOptions {
            encoding_config: Some(EncodingConfig::new_lossy(60.0)),
            ..EncoderOptions::default()
        };
        let mut encoder = Encoder::new_with_options((width, height), options).unwrap();
        let mut mark_ms = 0;
        for (index, duration_ms) in durations_ms.iter().enumerate() {
            let tone = (index * 120 % 250) as u8;
            encoder
                .add_frame(&frame(width, height, tone), mark_ms)
                .unwrap();
            mark_ms += duration_ms;
        }
        encoder.finalize(mark_ms).unwrap().to_vec()
    }

    fn animated() -> Vec<u8> {
        animated_with(512, 512, &[200, 300, 250])
    }

    fn static_image() -> Vec<u8> {
        let mut rgba = frame(512, 512, 200);
        rgba[..512 * 4 * 100].fill(0);
        encode_static_webp(&rgba, 512, 70).unwrap()
    }

    fn with_padding(mut webp: Vec<u8>, bytes: usize) -> Vec<u8> {
        let content = bytes - webp.len() - 8;
        webp.extend(b"XXXX");
        webp.extend((content as u32).to_le_bytes());
        webp.resize(bytes, 0);
        let riff = (bytes - 8) as u32;
        webp[4..8].copy_from_slice(&riff.to_le_bytes());
        webp
    }

    fn riff(chunks: &[u8]) -> Vec<u8> {
        let mut webp = b"RIFF".to_vec();
        webp.extend((chunks.len() as u32 + 4).to_le_bytes());
        webp.extend(b"WEBP");
        webp.extend(chunks);
        webp
    }

    #[test]
    fn animated_and_static_valid_pass() {
        let webp = animated();
        assert_eq!(
            inspect_whatsapp_sticker(&webp).unwrap(),
            WhatsappSticker {
                kind: StickerKind::Animated,
                size_bytes: webp.len() as u64,
                duration_ms: Some(750),
            }
        );
        let webp = static_image();
        assert_eq!(
            inspect_whatsapp_sticker(&webp).unwrap(),
            WhatsappSticker {
                kind: StickerKind::Static,
                size_bytes: webp.len() as u64,
                duration_ms: None,
            }
        );
    }

    #[test]
    fn static_vp8_and_vp8l_without_vp8x_pass() {
        let with_loss = encode_static_webp(&frame(512, 512, 10), 512, 70).unwrap();
        assert_eq!(&with_loss[12..16], b"VP8 ");
        let mut encoder = Encoder::new((512, 512)).unwrap();
        encoder.add_frame(&frame(512, 512, 10), 0).unwrap();
        let without_loss = encoder.finalize(100).unwrap().to_vec();
        assert_eq!(&without_loss[12..16], b"VP8L");
        for webp in [with_loss, without_loss] {
            let sticker = inspect_whatsapp_sticker(&webp).unwrap();
            assert_eq!(sticker.kind, StickerKind::Static);
        }
    }

    #[test]
    fn just_in_limits_pass() {
        let heavy = with_padding(animated(), 500_000);
        assert!(inspect_whatsapp_sticker(&heavy).is_ok());
        let heavy = with_padding(static_image(), 100_000);
        assert!(inspect_whatsapp_sticker(&heavy).is_ok());
        let long = animated_with(512, 512, &[5000, 5000]);
        let sticker = inspect_whatsapp_sticker(&long).unwrap();
        assert_eq!(sticker.duration_ms, Some(10_000));
        assert!(inspect_whatsapp_sticker(&animated_with(512, 512, &[8, 8])).is_ok());
    }

    #[test]
    fn side_that_is_not_512_rejects() {
        let static_image = encode_static_webp(&frame(256, 256, 1), 256, 70).unwrap();
        for (webp, sides) in [
            (animated_with(511, 512, &[100, 100]), (511, 512)),
            (static_image, (256, 256)),
        ] {
            let error = inspect_whatsapp_sticker(&webp).unwrap_err();
            assert!(
                matches!(error, StickerError::IncorrectSide { width, height } if (width, height) == sides),
                "{error:?}"
            );
        }
    }

    #[test]
    fn header_huge_rejects_without_decode() {
        let mut vp8x = animated();
        vp8x[24..30].copy_from_slice(&[0xff, 0x3f, 0, 0xff, 0x3f, 0]);

        let mut vp8 = b"VP8 \x0a\0\0\0\x10\0\0\x9d\x01\x2a".to_vec();
        vp8.extend([0xff, 0x3f, 0xff, 0x3f]);

        let mut vp8l = b"VP8L\x06\0\0\0\x2f".to_vec();
        vp8l.extend(0x0fff_ffffu32.to_le_bytes());
        vp8l.push(0);

        for (webp, side) in [(vp8x, 16_384), (riff(&vp8), 16_383), (riff(&vp8l), 16_384)] {
            let error = inspect_whatsapp_sticker(&webp).unwrap_err();
            assert!(
                matches!(error, StickerError::IncorrectSide { width, height } if (width, height) == (side, side)),
                "{error:?}"
            );
            assert!(matches!(
                tray_icon(&webp),
                Err(StickerError::IncorrectSide { .. })
            ));
        }
    }

    #[test]
    fn frame_that_declares_more_that_canvas_not_decodes() {
        let mut vp8l = b"VP8L\x06\0\0\0\x2f".to_vec();
        vp8l.extend(0x0fff_ffffu32.to_le_bytes());
        vp8l.push(0);
        let mut chunks = b"VP8X\x0a\0\0\0\x02\0\0\0\xff\x01\0\xff\x01\0".to_vec();
        chunks.extend(b"ANIM\x06\0\0\0\xff\xff\xff\xff\0\0");
        for _ in 0..2 {
            chunks.extend(b"ANMF");
            chunks.extend((16 + vp8l.len() as u32).to_le_bytes());
            chunks.extend([0, 0, 0, 0, 0, 0, 0xff, 0x3f, 0, 0xff, 0x3f, 0, 100, 0, 0, 0]);
            chunks.extend(&vp8l);
        }
        let error = inspect_whatsapp_sticker(&riff(&chunks)).unwrap_err();
        assert!(
            matches!(error, StickerError::Unreadable { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn container_animated_of_only_frame_rejects() {
        let frame = &static_image()[12..];
        let mut chunks = b"VP8X\x0a\0\0\0\x12\0\0\0\xff\x01\0\xff\x01\0".to_vec();
        chunks.extend(b"ANIM\x06\0\0\0\xff\xff\xff\xff\0\0");
        chunks.extend(b"ANMF");
        chunks.extend((16 + frame.len() as u32).to_le_bytes());
        chunks.extend([0, 0, 0, 0, 0, 0, 0xff, 0x01, 0, 0xff, 0x01, 0, 100, 0, 0, 0]);
        chunks.extend(frame);
        let error = inspect_whatsapp_sticker(&riff(&chunks)).unwrap_err();
        assert!(
            matches!(error, StickerError::AnimatedSingleFrame),
            "{error:?}"
        );
    }

    #[test]
    fn size_max_depends_of_kind() {
        let error = inspect_whatsapp_sticker(&with_padding(animated(), 501_000)).unwrap_err();
        assert!(
            matches!(
                error,
                StickerError::TooHeavy {
                    bytes: 501_000,
                    max: 500_000
                }
            ),
            "{error:?}"
        );
        let error = inspect_whatsapp_sticker(&with_padding(static_image(), 101_000)).unwrap_err();
        assert!(
            matches!(
                error,
                StickerError::TooHeavy {
                    bytes: 101_000,
                    max: 100_000
                }
            ),
            "{error:?}"
        );
        assert!(matches!(
            inspect_whatsapp_sticker(&vec![0; 500_001]),
            Err(StickerError::TooHeavy { .. })
        ));
    }

    #[test]
    fn animated_of_10_5_s_or_with_frame_of_5_ms_rejects() {
        let error = inspect_whatsapp_sticker(&animated_with(512, 512, &[5000, 5500])).unwrap_err();
        assert!(
            matches!(
                error,
                StickerError::TooLong {
                    duration_ms: 10_500
                }
            ),
            "{error:?}"
        );
        let error = inspect_whatsapp_sticker(&animated_with(512, 512, &[100, 5, 100])).unwrap_err();
        assert!(
            matches!(error, StickerError::FrameTooShort { duration_ms: 5 }),
            "{error:?}"
        );
    }

    #[test]
    fn that_is_not_webp_rejects() {
        for bytes in [
            &b""[..],
            b"RIFF",
            b"hola, no soy un sticker",
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
            b"RIFF\x10\0\0\0WAVEfmt \0\0\0\0",
        ] {
            assert!(
                matches!(inspect_whatsapp_sticker(bytes), Err(StickerError::NotWebp)),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn file_truncated_is_unreadable() {
        for webp in [animated(), static_image()] {
            for long in [12, 20, 30, webp.len() / 2, webp.len() - 2] {
                let error = inspect_whatsapp_sticker(&webp[..long]).unwrap_err();
                assert!(
                    matches!(error, StickerError::Unreadable { .. }),
                    "{long} de {}: {error:?}",
                    webp.len()
                );
            }
        }
    }

    #[test]
    fn frame_damaged_does_not_pass_as_sticker_more_short() {
        let mut webp = animated();
        let last = webp.windows(4).rposition(|w| w == b"ANMF").unwrap();
        webp[last + 8 + 16 + 8 + 10..].fill(0xff);
        let error = inspect_whatsapp_sticker(&webp).unwrap_err();
        assert!(
            matches!(error, StickerError::Unreadable { .. }),
            "{error:?}"
        );
    }

    fn sides_y_pixels(png: &[u8]) -> ((u32, u32), Vec<u8>) {
        let mut reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        ((info.width, info.height), rgba)
    }

    #[test]
    fn icon_of_animated_and_of_static_is_png_of_96() {
        for webp in [animated(), static_image()] {
            let icon = tray_icon(&webp).unwrap();
            assert!(icon.len() as u64 <= TRAY_ICON_MAX_SIZE);
            assert_eq!(sides_y_pixels(&icon).0, (96, 96));
            inspect_tray_icon(&icon).unwrap();
        }
        let (_, rgba) = sides_y_pixels(&tray_icon(&static_image()).unwrap());
        assert_eq!(rgba[3], 0);
        assert_eq!(rgba[rgba.len() - 1], 255);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![90; (width * height * 3) as usize])
            .unwrap();
        png
    }

    #[test]
    fn icon_imported_is_png_of_96_and_as_much_50_kb() {
        inspect_tray_icon(&png(96, 96)).unwrap();

        let mut heavy = png(96, 96);
        heavy.resize(50_001, 0);
        let mut huge = png(96, 96);
        huge[16..24].copy_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0]);
        let mut damaged = png(96, 96);
        damaged.truncate(45);
        for (name, png) in [
            ("95x96", png(95, 96)),
            ("pesado", heavy),
            ("cabecera de 65536", huge),
            ("dañado", damaged),
            ("vacío", Vec::new()),
            ("solo la firma", b"\x89PNG\r\n\x1a\n".to_vec()),
            ("un WebP", static_image()),
        ] {
            assert!(
                matches!(
                    inspect_tray_icon(&png),
                    Err(StickerError::IconNotValid { .. })
                ),
                "{name}"
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn bytes_any_never_give_panic(
            base in 0usize..4,
            changes in proptest::collection::vec((0.0f64..1.0, any::<u8>()), 1..12),
            cut in proptest::option::of(0.0f64..1.0),
        ) {
            static BASES: std::sync::LazyLock<[Vec<u8>; 4]> = std::sync::LazyLock::new(|| {
                let icon = tray_icon(&static_image()).unwrap();
                [animated(), static_image(), icon, vec![0; 64]]
            });
            let mut bytes = BASES[base].clone();
            let long = bytes.len();
            for (position, value) in changes {
                let until = if value % 2 == 0 { long.min(64) } else { long };
                bytes[(position * until as f64) as usize] = value;
            }
            if let Some(cut) = cut {
                bytes.truncate((cut * long as f64) as usize);
            }
            if let Ok(sticker) = inspect_whatsapp_sticker(&bytes) {
                prop_assert!(sticker.size_bytes <= 500_000);
            }
            if let Ok(icon) = tray_icon(&bytes) {
                prop_assert!(inspect_tray_icon(&icon).is_ok());
            }
            let _: Result<(), StickerError> = inspect_tray_icon(&bytes);
        }
    }
}
