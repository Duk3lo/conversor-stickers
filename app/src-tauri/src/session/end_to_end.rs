//! End-to-end tests: session and library with a real `ffmpeg`, without Tauri.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::Session;
use crate::contract::{
    CreateRequest, EntryState, FramingModeUi, FramingSpecUi, MediaOrigin, PriorityUi, ProgressUi,
    StaticRequest,
};
use crate::library::{Library, LibraryError, ValidSticker};
use crate::wastickers::{Imported, export, import};

fn ffmpeg(directory: &Path, name: &str, entry: &str, options: &[&str]) -> PathBuf {
    let path = directory.join(name);
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-f", "lavfi", "-i", entry])
        .args(options)
        .arg(&path)
        .output()
        .expect("hace falta ffmpeg para estas pruebas");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
}

fn image(directory: &Path, name: &str, size: &str) -> PathBuf {
    ffmpeg(
        directory,
        name,
        &format!("testsrc2=size={size}:rate=1"),
        &["-frames:v", "1"],
    )
}

fn video(directory: &Path) -> PathBuf {
    ffmpeg(
        directory,
        "video.mp4",
        "testsrc2=size=320x240:rate=30",
        &[
            "-t", "3", "-c:v", "mpeg4", "-q:v", "3", "-pix_fmt", "yuv420p",
        ],
    )
}

fn static_sticker(session: &Session, path: PathBuf, framing: FramingSpecUi) -> Vec<u8> {
    let entries = session.add_images(vec![path]);
    assert_eq!(entries[0].state, EntryState::Ready);
    let id = entries[0].id;
    let result = session
        .create_static(&StaticRequest { id, framing })
        .unwrap();
    assert!(result.meets_limit, "{result:?}");
    session.result(id).unwrap()
}

fn temp_files(session: &Session) -> Vec<PathBuf> {
    let state = session.state();
    let Some(temp) = &state.temp else {
        return Vec::new();
    };
    fs::read_dir(temp.path())
        .unwrap()
        .map(|file| file.unwrap().path())
        .collect()
}

#[test]
fn three_statics_export_and_import_with_same_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let session = Session::default();
    let mut origin = Library::open(&directory.path().join("origen")).unwrap();
    let pack = origin.create_pack("Mis stickers", None).unwrap();
    let mut created = Vec::new();
    for (name, size) in [
        ("a.png", "300x200"),
        ("b.png", "200x300"),
        ("c.png", "256x256"),
    ] {
        let bytes = static_sticker(
            &session,
            image(directory.path(), name, size),
            FramingSpecUi::default(),
        );
        origin.add_sticker(pack, &bytes, MediaOrigin::Own).unwrap();
        created.push(bytes);
    }

    let wastickers = export(&origin.to_export(pack).unwrap()).unwrap();
    let imported = import(
        &wastickers,
        "mis-stickers.wastickers",
        &crate::mobile::Executables::default(),
    )
    .unwrap();
    let mut destination = Library::open(&directory.path().join("destino")).unwrap();
    let new = destination.import(&imported).unwrap();

    let returned = destination.to_export(new).unwrap();
    assert_eq!(returned.title, "Mis stickers");
    assert_eq!(returned.stickers, created);
    drop(destination);
    let reopened = Library::open(&directory.path().join("destino")).unwrap();
    assert_eq!(reopened.to_export(new).unwrap().stickers, created);
}

#[test]
fn video_with_speed_and_priority_is_added_to_animated_pack() {
    let directory = tempfile::tempdir().unwrap();
    let session = Session::default();
    let entries = session.add_files(vec![video(directory.path())]);
    assert_eq!(entries[0].state, EntryState::Ready);
    let id = entries[0].id;
    let request = CreateRequest {
        id,
        preset: "whatsapp-sticker".to_string(),
        start_s: 0.5,
        seconds: 2.0,
        speed: 2.0,
        priority: PriorityUi::Smoothness,
        fps: None,
        side_px: None,
        framing: FramingSpecUi::default(),
    };

    let result = session.create(&request, &|_: ProgressUi| {}).unwrap();

    assert!(result.meets_limit, "{result:?}");
    let duration = result.duration_s.unwrap();
    assert!((duration - 1.0).abs() < 0.1, "{result:?}");
    let mut library = Library::open(directory.path().join("bib").as_path()).unwrap();
    let pack = library.create_pack("Animados", None).unwrap();
    library
        .add_sticker(pack, &session.result(id).unwrap(), MediaOrigin::Own)
        .unwrap();
    assert_eq!(library.ui().packs[0].stickers.len(), 1);
}

#[test]
fn static_in_animated_pack_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let session = Session::default();
    let entries = session.add_files(vec![video(directory.path())]);
    let id = entries[0].id;
    let request = CreateRequest {
        id,
        preset: "whatsapp-sticker".to_string(),
        start_s: 0.0,
        seconds: 1.0,
        speed: 1.0,
        priority: PriorityUi::Balance,
        fps: None,
        side_px: None,
        framing: FramingSpecUi::default(),
    };
    session.create(&request, &|_: ProgressUi| {}).unwrap();
    let mut library = Library::open(&directory.path().join("bib")).unwrap();
    let pack = library.create_pack("Animados", None).unwrap();
    library
        .add_sticker(pack, &session.result(id).unwrap(), MediaOrigin::Own)
        .unwrap();
    let static_image = static_sticker(
        &session,
        image(directory.path(), "e.png", "100x100"),
        FramingSpecUi {
            mode: FramingModeUi::Fit,
            ..FramingSpecUi::default()
        },
    );

    let error = library
        .add_sticker(pack, &static_image, MediaOrigin::Own)
        .unwrap_err();

    assert!(matches!(error, LibraryError::MixedKind { .. }), "{error:?}");
    assert_eq!(library.ui().packs[0].stickers.len(), 1);
}

#[test]
fn pasting_image_converts_to_sticker_and_removing_it_deletes_temp() {
    let directory = tempfile::tempdir().unwrap();
    let png = fs::read(image(directory.path(), "p.png", "320x200")).unwrap();
    let session = Session::default();

    let entry = session.paste_bytes(&png).unwrap();

    assert_eq!(entry.state, EntryState::Ready);
    assert_eq!(temp_files(&session).len(), 1);
    session
        .create_static(&StaticRequest {
            id: entry.id,
            framing: FramingSpecUi::default(),
        })
        .unwrap();
    let bytes = session.result(entry.id).unwrap();
    let mut library = Library::open(&directory.path().join("bib")).unwrap();
    let pack = library.create_pack("Pegados", None).unwrap();
    library.add_sticker(pack, &bytes, MediaOrigin::Own).unwrap();
    assert!(ValidSticker::new(bytes).is_ok());

    assert!(session.remove_entry(entry.id).unwrap().is_none());
    assert!(temp_files(&session).is_empty());
}

#[test]
fn closing_with_pasted_item_in_queue_leaves_no_temp_files() {
    let directory = tempfile::tempdir().unwrap();
    let png = fs::read(image(directory.path(), "p.png", "64x64")).unwrap();
    let session = Session::default();
    session.paste_bytes(&png).unwrap();
    let folder = temp_files(&session)[0].parent().unwrap().to_path_buf();
    assert!(folder.exists());

    session.close();

    assert!(!folder.exists());
}

#[test]
fn import_without_valid_stickers_creates_no_pack() {
    let directory = tempfile::tempdir().unwrap();
    let mut library = Library::open(&directory.path().join("bib")).unwrap();
    let empty = Imported {
        title: "Vacío".into(),
        author: String::new(),
        icon: None,
        stickers: Vec::new(),
        emojis: Vec::new(),
        notices: Vec::new(),
    };

    assert!(library.import(&empty).is_err());
    assert!(library.ui().packs.is_empty());
}
