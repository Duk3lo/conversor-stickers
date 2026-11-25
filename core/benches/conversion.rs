#![allow(clippy::panic, clippy::expect_used)]

#[path = "../tests/common/mod.rs"]
mod common;

use std::fs;

use criterion::{Criterion, criterion_group, criterion_main};

use conversor_core::{
    ConversionOptions, ConversionParams, DISCORD_GIF, FfmpegDecoder, Tracking, WHATSAPP_STICKER,
    convert,
};

use common::{Content, TestVideo};

fn conversion(criterion: &mut Criterion) {
    let video = TestVideo::new(Content::Pattern, 1280, 720, 10.0).path();
    let directory = tempfile::tempdir().expect("directorio temporal");
    let mut group = criterion.benchmark_group("convertir");
    group.sample_size(10);
    for preset in [WHATSAPP_STICKER, DISCORD_GIF] {
        let options = ConversionOptions::new(&ConversionParams::for_preset(preset))
            .expect("opciones por defecto válidas");
        let output = directory
            .path()
            .join(format!("salida.{}", preset.format.extension()));
        group.bench_function(preset.name, |measurement| {
            measurement.iter(|| {
                let report = convert(&FfmpegDecoder::new(&video), &options, Tracking::none())
                    .unwrap_or_else(|error| panic!("{preset}: {error}"));
                fs::write(&output, &report.bytes)
                    .unwrap_or_else(|error| panic!("{preset}: {error}"));
            });
        });
    }
    group.finish();
}

criterion_group!(benches, conversion);
criterion_main!(benches);
