//! Download of direct links to image or video files, without `yt-dlp`. Network data is untrusted:
//! only `https://`, size limit while streaming, and the kind is checked by the `Content-Type`
//! header and by the first bytes of the file.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use thiserror::Error;

const EXTENSIONS: [&str; 7] = ["mp4", "webm", "gif", "webp", "png", "jpg", "jpeg"];
pub(crate) const MAX_SIZE_BYTES: u64 = 100 * 1024 * 1024;
const TIME_LIMIT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 3;
const HEADER_SIZE: usize = 16;
const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, Error)]
pub(crate) enum DirectLinkError {
    #[error("El enlace no es una dirección https válida: {url}")]
    DisallowedUrl { url: String },
    #[error("No se pudo descargar el enlace: {reason}")]
    Network { reason: String },
    #[error("El enlace respondió {state}")]
    Http { state: u16 },
    #[error("El enlace redirige a una dirección no permitida o da demasiadas vueltas")]
    DisallowedRedirect,
    #[error("El enlace no es una imagen ni un vídeo (tipo {kind})")]
    DisallowedKind { kind: String },
    #[error("El archivo del enlace no es un GIF, WebP, PNG, JPEG, MP4 ni WebM")]
    DisallowedContent,
    #[error("El archivo supera el tamaño máximo de 100 MB")]
    TooLarge,
    #[error("Descarga cancelada")]
    Cancelled,
    #[error("No se pudo guardar el archivo descargado: {0}")]
    Disk(#[from] std::io::Error),
}

#[derive(Clone, Copy)]
enum Format {
    Mp4,
    WebM,
    Gif,
    Webp,
    Png,
    Jpeg,
}

impl Format {
    fn extension(self) -> &'static str {
        match self {
            Format::Mp4 => "mp4",
            Format::WebM => "webm",
            Format::Gif => "gif",
            Format::Webp => "webp",
            Format::Png => "png",
            Format::Jpeg => "jpg",
        }
    }
}

pub(crate) fn is_direct_link(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, route)) = rest.split_once('/') else {
        return false;
    };
    if authority.is_empty() {
        return false;
    }
    let path = route.split(['?', '#']).next().unwrap_or_default();
    let Some((_, extension)) = path.rsplit_once('.') else {
        return false;
    };
    EXTENSIONS
        .iter()
        .any(|valid| extension.eq_ignore_ascii_case(valid))
}

fn allowed_url(url: &str) -> bool {
    url.starts_with("https://") || (cfg!(test) && url.starts_with("http://127.0.0.1"))
}

pub(crate) fn download_direct(
    url: &str,
    destination_dir: &Path,
    cancel: &AtomicBool,
) -> Result<PathBuf, DirectLinkError> {
    if !allowed_url(url) {
        return Err(DirectLinkError::DisallowedUrl {
            url: url.to_owned(),
        });
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(TIME_LIMIT))
        .timeout_recv_response(Some(TIME_LIMIT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = request(&agent, url)?;
    let state = response.status().as_u16();
    if !(200..=299).contains(&state) {
        return Err(DirectLinkError::Http { state });
    }
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| "sin Content-Type".to_owned());
    if !acceptable_kind(&kind) {
        return Err(DirectLinkError::DisallowedKind { kind });
    }
    let declared = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared.is_some_and(|bytes| bytes > MAX_SIZE_BYTES) {
        return Err(DirectLinkError::TooLarge);
    }

    let mut reader = response.body_mut().as_reader();
    let mut header = Vec::with_capacity(HEADER_SIZE);
    (&mut reader)
        .take(HEADER_SIZE as u64)
        .read_to_end(&mut header)
        .map_err(network)?;
    let format = format_of(&header).ok_or(DirectLinkError::DisallowedContent)?;

    let mut file = tempfile::Builder::new()
        .prefix("link-")
        .suffix(&format!(".{}", format.extension()))
        .tempfile_in(destination_dir)?;
    file.write_all(&header)?;
    copy_into(&mut reader, &mut file, header.len() as u64, cancel)?;
    file.into_temp_path()
        .keep()
        .map_err(|failure| DirectLinkError::Disk(failure.error))
}

fn request(
    agent: &ureq::Agent,
    url: &str,
) -> Result<ureq::http::Response<ureq::Body>, DirectLinkError> {
    let mut current = url.to_owned();
    for _ in 0..=MAX_REDIRECTS {
        let response = agent.get(&current).call().map_err(network_from_ureq)?;
        if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        current = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .filter(|destination| allowed_url(destination))
            .map(str::to_owned)
            .ok_or(DirectLinkError::DisallowedRedirect)?;
    }
    Err(DirectLinkError::DisallowedRedirect)
}

fn copy_into(
    reader: &mut impl Read,
    file: &mut impl Write,
    mut total: u64,
    cancel: &AtomicBool,
) -> Result<(), DirectLinkError> {
    let mut chunk = [0u8; CHUNK_SIZE];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DirectLinkError::Cancelled);
        }
        let read = reader.read(&mut chunk).map_err(network)?;
        if read == 0 {
            return Ok(());
        }
        total += read as u64;
        if total > MAX_SIZE_BYTES {
            return Err(DirectLinkError::TooLarge);
        }
        file.write_all(&chunk[..read])?;
    }
}

fn acceptable_kind(kind: &str) -> bool {
    let mime = kind.split(';').next().unwrap_or_default().trim();
    let mime = mime.to_ascii_lowercase();
    mime.starts_with("image/") || mime.starts_with("video/") || mime == "application/octet-stream"
}

fn format_of(header: &[u8]) -> Option<Format> {
    if header.get(4..8) == Some(b"ftyp") {
        Some(Format::Mp4)
    } else if header.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some(Format::WebM)
    } else if header.starts_with(b"GIF8") {
        Some(Format::Gif)
    } else if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP") {
        Some(Format::Webp)
    } else if header.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(Format::Png)
    } else if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else {
        None
    }
}

fn network(error: std::io::Error) -> DirectLinkError {
    DirectLinkError::Network {
        reason: error.to_string(),
    }
}

fn network_from_ureq(error: ureq::Error) -> DirectLinkError {
    DirectLinkError::Network {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    const MP4: &[u8] = b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2";

    fn without_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn serve_it(response: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/clip.mp4", listener.local_addr().unwrap());
        thread::spawn(move || {
            if let Ok((mut connection, _)) = listener.accept() {
                let mut reader = BufReader::new(connection.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let _ = connection.write_all(&response);
            }
        });
        url
    }

    fn response(kind: &str, body: &[u8]) -> Vec<u8> {
        let mut bytes =
            format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nConnection: close\r\n\r\n")
                .into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn good_mp4_is_saved_with_its_bytes() {
        let url = serve_it(response("video/mp4", MP4));
        let destination = tempfile::tempdir().unwrap();

        let path = download_direct(&url, destination.path(), &without_cancel()).unwrap();

        assert_eq!(path.extension().unwrap(), "mp4");
        assert_eq!(fs::read(&path).unwrap(), MP4);
    }

    #[test]
    fn html_disguised_as_mp4_returns_error_and_leaves_nothing() {
        let html = b"<!doctype html><html><body>Inicia sesion</body></html>";
        let url = serve_it(response("video/mp4", html));
        let destination = tempfile::tempdir().unwrap();

        let error = download_direct(&url, destination.path(), &without_cancel()).unwrap_err();

        assert!(matches!(error, DirectLinkError::DisallowedContent));
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
    }

    #[test]
    fn html_with_its_type_returns_kind_error() {
        let url = serve_it(response("text/html", b"<html></html>"));
        let destination = tempfile::tempdir().unwrap();

        let error = download_direct(&url, destination.path(), &without_cancel()).unwrap_err();

        assert!(matches!(error, DirectLinkError::DisallowedKind { .. }));
    }

    #[test]
    fn file_declaring_too_large_is_rejected_before_downloading() {
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_SIZE_BYTES + 1
        );
        let url = serve_it(header.into_bytes());
        let destination = tempfile::tempdir().unwrap();

        let error = download_direct(&url, destination.path(), &without_cancel()).unwrap_err();

        assert!(matches!(error, DirectLinkError::TooLarge));
    }

    #[test]
    fn body_over_limit_is_cut_and_leaves_nothing() {
        let mut body = MP4.to_vec();
        body.resize(MAX_SIZE_BYTES as usize + 1, 0);
        let url = serve_it(response("video/mp4", &body));
        let destination = tempfile::tempdir().unwrap();

        let error = download_direct(&url, destination.path(), &without_cancel()).unwrap_err();

        assert!(matches!(error, DirectLinkError::TooLarge));
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
    }

    #[test]
    fn external_http_is_not_downloaded() {
        let destination = tempfile::tempdir().unwrap();

        let error = download_direct(
            "http://ejemplo.org/clip.mp4",
            destination.path(),
            &without_cancel(),
        )
        .unwrap_err();

        assert!(matches!(error, DirectLinkError::DisallowedUrl { .. }));
    }

    #[test]
    fn only_https_with_image_or_video_extension_is_direct() {
        assert!(is_direct_link("https://cdn.ejemplo.org/a/clip.mp4"));
        assert!(is_direct_link(
            "https://cdn.ejemplo.org/a/CLIP.WebM?token=1"
        ));
        assert!(is_direct_link("https://cdn.ejemplo.org/gato.jpeg#x"));
        assert!(!is_direct_link("http://cdn.ejemplo.org/clip.mp4"));
        assert!(!is_direct_link("https://cdn.ejemplo.org/pagina"));
        assert!(!is_direct_link("https://cdn.ejemplo.org/?f=clip.mp4"));
        assert!(!is_direct_link("https://cdn.ejemplo.org/clip.mp4.html"));
        assert!(!is_direct_link("https:///clip.mp4"));
    }
}
