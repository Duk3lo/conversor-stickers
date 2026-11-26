//! Desktop and Android app: one window that converts videos into stickers or GIFs with the core
//! and stores them in packs. It is a library so Android can load it as a `cdylib`; `main.rs`
//! only launches it on desktop.

#[cfg(edition = "github")]
mod bot;
mod commands;
#[cfg(edition = "github")]
mod commands_bot;
mod commands_community;
#[cfg(edition = "github")]
mod commands_extensions;
mod commands_gifs;
mod commands_mobile;
mod commands_packs;
mod commands_video;
mod community;
mod contract;
#[cfg(edition = "github")]
mod contract_bot;
mod contract_community;
mod contract_gifs;
mod direct_link;
mod environment;
#[cfg(edition = "github")]
mod extensions;
mod external_sticker;
mod gif_providers;
mod gifs;
mod library;
mod mobile;
mod session;
mod storage;
mod wastickers;
mod whatsapp;

use std::sync::Arc;

use tauri::{Manager, RunEvent};
use tracing::{error, warn};
use tracing_subscriber::EnvFilter;

use crate::commands_community::CommunityState;
use crate::commands_gifs::GifsState;
use crate::commands_packs::{LibraryState, library_root};
use crate::library::Library;
use crate::session::Session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into());
    // At `trace` the HTTP parser dumps raw bytes, which for the bot include the pairing code and QR.
    #[cfg(edition = "github")]
    let filter = match "ureq_proto=warn".parse() {
        Ok(directive) => filter.add_directive(directive),
        Err(_) => filter,
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
    environment::prepare_webview();

    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // Only Rust uses it to open `content://` URIs; the webview has no `fs:*` permission.
    #[cfg(target_os = "android")]
    let builder = builder.plugin(tauri_plugin_fs::init());
    let app = builder
        .plugin(mobile::bridge_plugin())
        .manage(Arc::new(Session::new()))
        .setup(|app| {
            mobile::prepare_temp(app.handle());
            mobile::check_executables_on_startup(app.handle());
            let root = library_root(&app.path().app_data_dir()?);
            // Only pack stickers and icons are visible through the asset protocol; the static scope
            // stays empty.
            app.asset_protocol_scope()
                .allow_directory(Library::packs_directory(&root), true)?;
            // Community downloads are visible through the asset protocol like library stickers.
            let community_cache = app.path().app_cache_dir()?.join("community");
            app.asset_protocol_scope()
                .allow_directory(&community_cache, true)?;
            #[cfg(edition = "github")]
            app.manage(Arc::new(bot::ConnectionStore::new(bot::connection_file(
                &app.path().app_data_dir()?,
            ))));
            app.manage(Arc::new(CommunityState::new(
                community_cache,
                app.path().app_data_dir()?,
            )?));
            // Thumbnails of searched GIFs are visible through the asset protocol like community ones.
            let gifs_cache = app.path().app_cache_dir()?.join("gifs");
            app.asset_protocol_scope()
                .allow_directory(gifs_cache.join("thumbnails"), true)?;
            app.asset_protocol_scope()
                .allow_directory(gifs_cache.join("stickers").join("thumbnails"), true)?;
            app.manage(Arc::new(GifsState::new(
                gifs_cache,
                app.path().app_config_dir()?,
            )));
            let library = Arc::new(LibraryState::new(root));
            app.manage(Arc::clone(&library));
            commands_mobile::prune_on_startup(app.path().app_data_dir()?, library);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::presets,
            commands::pick_from_gallery,
            commands::paste,
            #[cfg(edition = "github")]
            commands::add_links,
            #[cfg(edition = "github")]
            commands::download_queue,
            #[cfg(edition = "github")]
            commands::cancel_downloads,
            commands::remove_entry,
            commands::preview,
            commands::timeline_thumbnails,
            commands::cancel_preview,
            commands::create,
            commands::cancel_creation,
            commands::estimate,
            commands::create_static,
            commands::result,
            commands::save,
            commands_video::save_video,
            commands_video::save_sound,
            commands_video::save_clip_video,
            commands_packs::library,
            commands_packs::create_category,
            commands_packs::rename_category,
            commands_packs::delete_category,
            commands_packs::create_pack,
            commands_packs::edit_pack,
            commands_packs::delete_pack,
            commands_packs::add_sticker,
            commands_packs::remove_sticker,
            commands_packs::use_as_icon,
            commands_packs::set_emojis,
            commands_packs::mark_favorite,
            commands_packs::reorder_stickers,
            commands_packs::create_pack_from_favorites,
            commands_packs::export_pack,
            commands_packs::export_sticker,
            commands_packs::export_backup,
            commands_packs::restore_backup,
            commands_packs::import_wastickers,
            #[cfg(edition = "github")]
            commands_extensions::install_extension,
            commands_community::catalog,
            commands_community::community_detail,
            commands_community::accept_community_notice,
            commands_community::download_from_community,
            commands_community::cancel_community_download,
            commands_community::propose_to_community,
            commands_gifs::gif_provider,
            commands_gifs::configure_gif_provider,
            commands_gifs::search_gifs,
            commands_gifs::use_gif,
            commands_gifs::klipy_stickers,
            commands_gifs::klipy_categories,
            commands_gifs::add_klipy_sticker,
            commands_mobile::add_to_whatsapp,
            commands_mobile::paste_from_clipboard,
            commands_mobile::preview_path,
            commands_mobile::receive_shared,
            commands_mobile::share_sticker,
            commands_mobile::share_result,
            commands_mobile::send_result_to_whatsapp,
            commands_mobile::send_sticker_to_whatsapp,
            commands_mobile::open_saved_folder,
            commands_mobile::open_whatsapp,
            #[cfg(edition = "github")]
            commands_mobile::send_to_bot,
            #[cfg(edition = "github")]
            commands_bot::bot_connection,
            #[cfg(edition = "github")]
            commands_bot::bot_connect,
            #[cfg(edition = "github")]
            commands_bot::bot_disable,
            #[cfg(edition = "github")]
            commands_bot::bot_status,
            #[cfg(edition = "github")]
            commands_bot::bot_activate,
            #[cfg(edition = "github")]
            commands_bot::bot_pause,
            #[cfg(edition = "github")]
            commands_bot::bot_link,
            #[cfg(edition = "github")]
            commands_bot::bot_unlink,
            #[cfg(edition = "github")]
            commands_bot::bot_open_chat,
        ])
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(failure) => {
            error!(%failure, "la app no pudo arrancar");
            std::process::exit(1);
        }
    };
    // A signal would kill the process without `RunEvent::Exit` and leave `yt-dlp` (its own process
    // group) alive: exit as when closing the window.
    let output = app.handle().clone();
    if let Err(failure) = ctrlc::set_handler(move || output.exit(0)) {
        warn!(%failure, "no se pudo preparar el cierre por señales; Ctrl-C no parará las descargas");
    }
    app.run(|app, event| {
        // Managed state is not dropped on exit: close the session by hand to stop `yt-dlp` and delete
        // the temp files.
        if let RunEvent::Exit = event {
            app.state::<Arc<Session>>().close();
        }
    });
}
