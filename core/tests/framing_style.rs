#![allow(clippy::panic, clippy::expect_used)]

mod common;

use std::fs;
use std::sync::atomic::AtomicBool;

use conversor_core::{
    Align, Background, ConversionOptions, ConversionParams, Decoder, FfmpegDecoder, FramingMode,
    FramingSpec, Shape, Tracking, WHATSAPP_STICKER, convert, plan_time,
};

use common::{Content, TestVideo, check_duration_except_rules};

const SIDE: u32 = 512;

/// 640x360 video, red left half and blue right half: fitted in 512 it covers rows 112..400.
fn video() -> TestVideo {
    TestVideo::new(Content::Halves, 640, 360, 1.0)
}

fn decode(framing: FramingSpec) -> Vec<u8> {
    let decoder = FfmpegDecoder::new(video().path()).with_framing(framing);
    let options = ConversionOptions::new(&ConversionParams::for_preset(WHATSAPP_STICKER))
        .expect("parámetros válidos");
    let info = decoder.inspect().expect("el video de prueba es válido");
    let plan = plan_time(info.duration_s, &options).expect("el tramo entra");
    let clip = decoder
        .decode(&plan, options.fps(), SIDE, &AtomicBool::new(false))
        .expect("el video se decodifica");
    clip.frames()[0].rgba.to_vec()
}

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let start = ((y * SIDE + x) * 4) as usize;
    rgba[start..start + 4]
        .try_into()
        .expect("un píxel son 4 bytes")
}

fn is_red([r, g, b, a]: [u8; 4]) -> bool {
    a == 255 && r > 180 && g < 80 && b < 80
}

fn is_blue([r, g, b, a]: [u8; 4]) -> bool {
    a == 255 && b > 180 && r < 80 && g < 80
}

fn is_green([r, g, b, a]: [u8; 4]) -> bool {
    a == 255 && g > 180 && r < 80 && b < 80
}

fn transparent(pixel: [u8; 4]) -> bool {
    pixel[3] == 0
}

fn fit_with(background: Background, align: Align) -> FramingSpec {
    FramingSpec::fit()
        .with_style(background, Shape::Square, 1.0, align)
        .expect("estilo válido")
}

fn crop_with(shape: Shape, scale: f64) -> FramingSpec {
    FramingSpec::new(FramingMode::Crop, 0.5, 0.5, 1.0)
        .and_then(|crop| crop.with_style(Background::Transparent, shape, scale, Align::Center))
        .expect("estilo válido")
}

#[test]
fn fit_aligns_the_frame_to_the_top_the_centre_or_the_bottom() {
    let top = decode(fit_with(Background::Transparent, Align::Top));
    assert!(is_red(pixel(&top, 100, 10)) && is_blue(pixel(&top, 400, 280)));
    assert!(transparent(pixel(&top, 100, 400)));

    let centre = decode(fit_with(Background::Transparent, Align::Center));
    assert!(transparent(pixel(&centre, 100, 50)) && transparent(pixel(&centre, 100, 460)));
    assert!(is_red(pixel(&centre, 100, 256)) && is_blue(pixel(&centre, 400, 256)));

    let bottom = decode(fit_with(Background::Transparent, Align::Bottom));
    assert!(transparent(pixel(&bottom, 100, 100)));
    assert!(is_red(pixel(&bottom, 100, 500)) && is_blue(pixel(&bottom, 400, 230)));
}

#[test]
fn fit_with_colour_background_fills_the_bands() {
    let green = Background::Color([0, 255, 0]);
    let rgba = decode(fit_with(green, Align::Center));
    assert!(is_green(pixel(&rgba, 256, 50)) && is_green(pixel(&rgba, 10, 500)));
    assert!(is_red(pixel(&rgba, 100, 256)) && is_blue(pixel(&rgba, 400, 256)));
}

#[test]
fn fit_with_blurred_background_fills_the_bands_with_the_video_colours() {
    let rgba = decode(fit_with(Background::Blur, Align::Center));
    let (left, right) = (pixel(&rgba, 40, 30), pixel(&rgba, 470, 30));
    assert_eq!((left[3], right[3]), (255, 255));
    assert!(left[0] > 150 && left[2] < 100, "{left:?}");
    assert!(right[2] > 150 && right[0] < 100, "{right:?}");
    assert!(is_red(pixel(&rgba, 100, 256)) && is_blue(pixel(&rgba, 400, 256)));
}

#[test]
fn circle_and_rounded_shapes_clear_the_corners_and_keep_the_edge_middles() {
    for shape in [Shape::Circle, Shape::Rounded] {
        let rgba = decode(crop_with(shape, 1.0));
        for (x, y) in [(1, 1), (510, 1), (1, 510), (510, 510), (10, 10)] {
            assert!(transparent(pixel(&rgba, x, y)), "{shape:?} ({x}, {y})");
        }
        for (x, y) in [(256, 4), (4, 256), (507, 256)] {
            assert_eq!(pixel(&rgba, x, y)[3], 255, "{shape:?} ({x}, {y})");
        }
        assert!(is_red(pixel(&rgba, 100, 256)) && is_blue(pixel(&rgba, 400, 256)));
    }
    // Only the circle clears a point that the rounded corner still draws.
    let circle = decode(crop_with(Shape::Circle, 1.0));
    let rounded = decode(crop_with(Shape::Rounded, 1.0));
    assert!(transparent(pixel(&circle, 60, 60)));
    assert_eq!(pixel(&rounded, 60, 60)[3], 255);
}

#[test]
fn scale_draws_the_content_smaller_with_a_transparent_margin() {
    let rgba = decode(crop_with(Shape::Square, 0.5));
    for (x, y) in [(10, 256), (126, 256), (385, 256), (256, 126), (256, 500)] {
        assert!(transparent(pixel(&rgba, x, y)), "({x}, {y})");
    }
    assert!(is_red(pixel(&rgba, 135, 256)) && is_blue(pixel(&rgba, 380, 256)));
    assert!(is_red(pixel(&rgba, 135, 135)) && is_blue(pixel(&rgba, 380, 380)));
}

#[test]
fn every_style_still_passes_the_whatsapp_rules() {
    let styles = [
        fit_with(Background::Blur, Align::Bottom)
            .with_style(Background::Blur, Shape::Circle, 0.8, Align::Bottom)
            .expect("estilo válido"),
        fit_with(Background::Color([255, 200, 0]), Align::Top)
            .with_style(
                Background::Color([255, 200, 0]),
                Shape::Rounded,
                0.6,
                Align::Top,
            )
            .expect("estilo válido"),
        crop_with(Shape::Circle, 0.5),
    ];
    let options = ConversionOptions::new(&ConversionParams::for_preset(WHATSAPP_STICKER))
        .expect("parámetros válidos");
    for framing in styles {
        let decoder = FfmpegDecoder::new(video().path()).with_framing(framing);
        let report = convert(&decoder, &options, Tracking::none()).expect("la conversión cabe");
        let directory = tempfile::tempdir().expect("directorio temporal");
        let path = directory.path().join("salida.webp");
        fs::write(&path, &report.bytes).expect("se puede escribir la salida");
        check_duration_except_rules(
            &WHATSAPP_STICKER,
            WHATSAPP_STICKER.max_size.bytes(),
            &report,
            &path,
        );
    }
}
