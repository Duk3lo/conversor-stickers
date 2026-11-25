#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;
use webp_animation::Encoder;

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_conversor-stickers"))
        .arg("catalog")
        .args(arguments)
        .output()
        .expect("se puede ejecutar la CLI")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("terminó con un código")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn sticker(tone: u8) -> Vec<u8> {
    let mut encoder = Encoder::new((512, 512)).expect("codificador");
    encoder
        .add_frame(&[tone, 80, 160, 255].repeat(512 * 512), 0)
        .expect("frame");
    encoder.finalize(100).expect("webp").to_vec()
}

fn icon() -> Vec<u8> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 96, 96);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("cabecera PNG");
    writer
        .write_image_data(&[200, 80, 40, 255].repeat(96 * 96))
        .expect("datos PNG");
    writer.finish().expect("PNG");
    png
}

fn catalog(license: &str) -> TempDir {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let root = directory.path();
    fs::write(
        root.join("categories.json"),
        r#"[{"id":"otros","name":"Otros"}]"#,
    )
    .expect("categories");
    fs::write(root.join("withdrawn.json"), "[]").expect("withdrawn");
    let folder = root.join("packs/colores-lisos");
    fs::create_dir_all(&folder).expect("carpeta del paquete");
    for n in 1..=3u8 {
        fs::write(folder.join(format!("{n:02}.webp")), sticker(n * 50)).expect("sticker");
    }
    fs::write(folder.join("icon.png"), icon()).expect("icon");
    let pack = format!(
        r#"{{"id":"colores-lisos","version":1,"name":"Colores lisos","author":"Ana",
        "license":"{license}","category":"otros","tags":["color"],"animated":false,
        "added":"2026-11-22","icon":{{"file":"icon.png"}},
        "stickers":[{{"file":"01.webp","emojis":["🙂"]}},
        {{"file":"02.webp","emojis":["🙂"]}},{{"file":"03.webp","emojis":["🙂"]}}]}}"#
    );
    fs::write(folder.join("pack.json"), pack).expect("pack.json");
    directory
}

fn path(directory: &Path) -> &str {
    directory.to_str().expect("ruta UTF-8")
}

#[test]
fn validate_catalog_good_exits_with_0() {
    let directory = catalog("CC0-1.0");
    let output = run(&["validate", path(directory.path())]);
    assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("1 paquetes"));
}

#[test]
fn validate_catalog_that_violates_rule_exits_with_3_and_says_which() {
    let directory = catalog("MIT");
    let output = run(&["validate", path(directory.path())]);
    assert_eq!(code(&output), 3);
    let error = text(&output.stderr);
    assert!(
        error.contains("colores-lisos") && error.contains("licencia"),
        "{error}"
    );
}

#[test]
fn validate_directory_that_does_not_exist_exits_with_1() {
    let directory = tempfile::tempdir().expect("directorio temporal");
    let output = run(&["validate", path(&directory.path().join("nada"))]);
    assert_eq!(code(&output), 1);
    assert!(text(&output.stderr).starts_with("error: "));
}

#[test]
fn index_writes_file_and_is_deterministic() {
    let directory = catalog("CC-BY-4.0");
    let output_dir = tempfile::tempdir().expect("directorio temporal");
    let first = output_dir.path().join("index.json");
    let second = output_dir.path().join("indice2.json");
    for file in [&first, &second] {
        let output = run(&["index", path(directory.path()), "--output", path(file)]);
        assert_eq!(code(&output), 0, "{}", text(&output.stderr));
    }
    let bytes = fs::read(&first).expect("índice");
    assert_eq!(bytes, fs::read(&second).expect("segundo índice"));
    let index = text(&bytes);
    assert!(index.contains("\"format\": 1") && index.contains("\"sha256\""));
}

#[test]
fn index_of_catalog_invalid_exits_with_3_and_writes_nothing() {
    let directory = catalog("MIT");
    let output_dir = tempfile::tempdir().expect("directorio temporal");
    let file = output_dir.path().join("index.json");
    let output = run(&["index", path(directory.path()), "--output", path(&file)]);
    assert_eq!(code(&output), 3);
    assert!(!file.exists());
}

#[test]
fn arguments_incorrect_exit_with_2_in_spanish() {
    let output = run(&["index", "algo"]);
    assert_eq!(code(&output), 2);
    assert!(text(&output.stderr).contains("falta un argumento obligatorio"));
    assert_eq!(code(&run(&["inventado"])), 2);
}

#[test]
fn help_exits_with_0() {
    let output = run(&["--help"]);
    assert_eq!(code(&output), 0);
    assert!(text(&output.stdout).contains("validate"));
}
