//! The Google Play edition (`play` feature) cannot download from sites with yt-dlp: it only accepts
//! direct links to image or video files, which `direct_link` downloads without any external tool.

use crate::contract::{UiError, UiErrorKind};
use crate::direct_link::is_direct_link;

/// Compile-time flag; the functions below take it as a parameter so tests cover both editions.
pub(crate) const PLAY: bool = cfg!(feature = "play");

const TYPED_LINK_MESSAGE: &str = "esta edición no descarga enlaces de TikTok, X, Instagram, YouTube y similares; solo enlaces https directos a una imagen o un video (.mp4, .webm, .gif, .webp, .png, .jpg). También puedes elegir el archivo de tu galería";
const SHARED_LINK_MESSAGE: &str = "esta edición no puede descargar enlaces compartidos de TikTok, X, Instagram, YouTube y similares; guarda el video en tu galería y compártelo o elígelo desde aquí";

/// Rejects every link that would need yt-dlp. Always passes in the full edition.
pub(crate) fn require_direct_links(play: bool, text: &str) -> Result<(), UiError> {
    check(play, text, TYPED_LINK_MESSAGE)
}

/// Same for links that arrive through Android's share sheet.
pub(crate) fn require_direct_shared_links(play: bool, links: &[String]) -> Result<(), UiError> {
    check(play, &links.join("\n"), SHARED_LINK_MESSAGE)
}

fn check(play: bool, text: &str, message: &str) -> Result<(), UiError> {
    if play && text.split_whitespace().any(|chunk| !is_direct_link(chunk)) {
        return Err(UiError::new(UiErrorKind::LinkNotValid, message));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_edition_accepts_any_link() {
        assert!(require_direct_links(false, "https://www.tiktok.com/@a/video/1").is_ok());
        assert!(require_direct_shared_links(false, &["https://youtu.be/x".to_string()]).is_ok());
    }

    #[test]
    fn play_edition_rejects_links_that_need_yt_dlp() {
        let error = require_direct_links(true, "https://www.tiktok.com/@a/video/1").unwrap_err();
        assert_eq!(error.kind, UiErrorKind::LinkNotValid);
        assert!(error.message.contains("esta edición"));
        assert!(require_direct_links(true, "https://a.example/x.mp4 https://youtu.be/x").is_err());
    }

    #[test]
    fn play_edition_accepts_direct_file_links() {
        assert!(
            require_direct_links(true, "https://a.example/x.mp4\nhttps://b.example/y.GIF?z=1")
                .is_ok()
        );
    }

    #[test]
    fn play_edition_rejects_shared_links_with_its_own_message() {
        let error = require_direct_shared_links(true, &["https://x.com/a/status/1".to_string()])
            .unwrap_err();
        assert!(error.message.contains("compartidos"));
        assert!(
            require_direct_shared_links(true, &["https://a.example/x.webm".to_string()]).is_ok()
        );
        assert!(require_direct_shared_links(true, &[]).is_ok());
    }
}
