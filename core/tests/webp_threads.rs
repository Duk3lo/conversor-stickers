#![allow(clippy::expect_used)]

use std::thread;

use conversor_core::{FramingSpec, RgbaImage, Tracking, WHATSAPP_STATIC_STICKER, convert_image};

const THREADS: usize = 32;

#[test]
fn many_threads_encode_to_time_in_process_new() {
    let side: u32 = 64;
    let rgba = (0..side * side)
        .flat_map(|i| {
            [
                (i % 251) as u8,
                (i / 7 % 241) as u8,
                (i / 13 % 239) as u8,
                255,
            ]
        })
        .collect();
    let image = RgbaImage::new(side, side, rgba).expect("imagen válida");

    let results: Vec<Vec<u8>> = thread::scope(|scope| {
        let handlers: Vec<_> = (0..THREADS)
            .map(|_| {
                scope.spawn(|| {
                    convert_image(
                        &image,
                        &WHATSAPP_STATIC_STICKER,
                        &FramingSpec::default(),
                        Tracking::none(),
                    )
                    .expect("la conversión no falla")
                    .bytes
                })
            })
            .collect();
        handlers
            .into_iter()
            .map(|handler| handler.join().expect("el hilo no entra en pánico"))
            .collect()
    });

    assert_eq!(results.len(), THREADS);
    assert!(results.windows(2).all(|par| par[0] == par[1]));
}
