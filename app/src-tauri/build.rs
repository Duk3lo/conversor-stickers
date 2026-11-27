// Declares the app commands so each one needs its `allow-<command>` permission in
// `capabilities/main.json`.
const COMMANDS: &[&str] = &[
    "presets",
    "pick_from_gallery",
    "paste",
    "remove_entry",
    "preview",
    "timeline_thumbnails",
    "video_still",
    "cancel_preview",
    "create",
    "cancel_creation",
    "estimate",
    "create_static",
    "result",
    "save",
    "save_video",
    "save_sound",
    "save_clip_video",
    "library",
    "create_category",
    "rename_category",
    "delete_category",
    "create_pack",
    "edit_pack",
    "delete_pack",
    "add_sticker",
    "remove_sticker",
    "use_as_icon",
    "set_emojis",
    "mark_favorite",
    "reorder_stickers",
    "create_pack_from_favorites",
    "export_pack",
    "export_sticker",
    "export_backup",
    "restore_backup",
    "import_wastickers",
    "catalog",
    "community_detail",
    "accept_community_notice",
    "download_from_community",
    "cancel_community_download",
    "propose_to_community",
    "gif_provider",
    "configure_gif_provider",
    "search_gifs",
    "use_gif",
    "klipy_stickers",
    "klipy_categories",
    "add_klipy_sticker",
    "add_to_whatsapp",
    "paste_from_clipboard",
    "preview_path",
    "receive_shared",
    "share_sticker",
    "share_result",
    "send_result_to_whatsapp",
    "send_sticker_to_whatsapp",
    "open_saved_folder",
    "open_whatsapp",
    "open_external",
];

// Only in the `github` edition (`cfg(edition = "github")`, set by `main`): extensions, the link
// downloader and the connection to the user's bot. In `play` they are neither declared nor granted.
const GITHUB_COMMANDS: &[&str] = &[
    "add_links",
    "download_queue",
    "cancel_downloads",
    "install_extension",
    "send_to_bot",
    "bot_connection",
    "bot_connect",
    "bot_connect_link",
    "bot_connect_local",
    "bot_receive_link",
    "bot_accept_link",
    "bot_reject_link",
    "bot_disable",
    "bot_status",
    "bot_activate",
    "bot_pause",
    "bot_link",
    "bot_unlink",
    "bot_open_chat",
];

// The KLIPY key is baked in as `BUNDLED_KLIPY_KEY` only if the build sets `KLIPY_KEY` explicitly
// (a public release must not: it uses `CONVERSOR_PROXY_URL` instead). Without a key the app still
// builds. A key with odd characters is discarded, and nothing here prints it.
fn bundle_klipy_key() {
    println!("cargo:rerun-if-env-changed=KLIPY_KEY");
    let key = std::env::var("KLIPY_KEY")
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty());
    let Some(key) = key else { return };
    let valid = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        println!("cargo:rustc-env=BUNDLED_KLIPY_KEY={key}");
    } else {
        println!("cargo:warning=la clave de KLIPY tiene caracteres no válidos y no se integra");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    bundle_klipy_key();
    // CONVERSOR_EDITION is the single switch (also read by Vite and Gradle): anything but `play`
    // is the `github` edition.
    println!("cargo:rerun-if-env-changed=CONVERSOR_EDITION");
    println!("cargo:rustc-check-cfg=cfg(edition, values(\"github\"))");
    let github = std::env::var("CONVERSOR_EDITION").as_deref() != Ok("play");
    if github {
        println!("cargo:rustc-cfg=edition=\"github\"");
    }
    let commands: &'static [&'static str] = if github {
        [COMMANDS, GITHUB_COMMANDS].concat().leak()
    } else {
        COMMANDS
    };
    let mut attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(commands));
    if !github {
        // Without recursion: `capabilities/github/` is not loaded.
        attributes = attributes.capabilities_path_pattern("./capabilities/*.json");
    }
    tauri_build::try_build(attributes)?;
    Ok(())
}
