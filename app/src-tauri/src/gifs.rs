//! GIF search client, without Tauri: KLIPY and GIPHY behind one trait. Network data is untrusted:
//! only `https` to each provider's hosts, responses limited while streaming and no redirects. The
//! key goes in the request URL, so no error or log includes API URLs.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use thiserror::Error;
use tracing::warn;

use crate::storage::write_atomic;

const TIME_LIMIT: Duration = Duration::from_secs(10);
const JSON_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const THUMBNAIL_LIMIT_BYTES: u64 = 1024 * 1024;
pub(crate) const MEDIA_LIMIT_BYTES: u64 = 30 * 1024 * 1024;
const PER_PAGE: u32 = 24;
const MAX_QUERY_LENGTH: usize = 100;
const MAX_KEY_LENGTH: usize = 128;
const GIPHY_MAX_OFFSET: u32 = 499;
const EXTENSIONS: &[&str] = &["gif", "mp4", "webp", "webm", "jpg", "png"];

const KLIPY_URL: &str = "https://api.klipy.com";
const KLIPY_HOSTS: &[&str] = &[
    "api.klipy.com",
    "static.klipy.com",
    "static1.klipy.com",
    "static2.klipy.com",
];
const GIPHY_URL: &str = "https://api.giphy.com";
const GIPHY_HOSTS: &[&str] = &[
    "api.giphy.com",
    "media.giphy.com",
    "media0.giphy.com",
    "media1.giphy.com",
    "media2.giphy.com",
    "media3.giphy.com",
    "media4.giphy.com",
    "i.giphy.com",
];

#[derive(Debug, Error)]
pub(crate) enum GifError {
    #[error("Falta la clave de {provider}")]
    MissingKey { provider: &'static str },

    #[error("La clave no es válida")]
    InvalidKey,

    #[error("La dirección del servicio de la app no es válida")]
    InvalidProxy,

    #[error("Límite de búsquedas: espera un minuto")]
    Limit,

    #[error("Sin conexión")]
    Offline,

    #[error("El servicio de GIF respondió con el error {state}")]
    Http { state: u16 },

    #[error("Una respuesta del servicio de GIF supera el tamaño máximo permitido")]
    TooLarge,

    #[error("El servicio de GIF respondió algo que no se entiende")]
    InvalidResponse,

    #[error("El servicio de GIF apunta a una dirección que no se admite")]
    DisallowedAddress,

    #[error("La búsqueda tiene que tener entre 1 y {MAX_QUERY_LENGTH} caracteres")]
    InvalidQuery,

    #[error("Ese GIF no está en la última búsqueda")]
    UnknownGif,

    #[error("Ese sticker no está en la última búsqueda")]
    UnknownSticker,

    #[error("No se pudo {action}: {source}")]
    Disk {
        action: &'static str,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GifResult {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) thumbnail_url: String,
    pub(crate) media_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GifPage {
    pub(crate) results: Vec<GifResult>,
    pub(crate) next: Option<u32>,
}

pub(crate) trait GifProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn attribution(&self) -> &'static str;
    fn find(&self, text: &str, page: u32) -> Result<GifPage, GifError>;
    fn trending(&self, page: u32) -> Result<GifPage, GifError>;
    fn download(&self, url: &str, max: u64) -> Result<Vec<u8>, GifError>;
}

struct Http {
    agent: ureq::Agent,
    hosts: &'static [&'static str],
    /// The app's proxy: allowed for API calls only, never for downloading media.
    proxy: Option<String>,
}

impl Http {
    fn new(hosts: &'static [&'static str]) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIME_LIMIT))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        Http {
            agent,
            hosts,
            proxy: None,
        }
    }

    fn with_proxy(mut self, proxy: &str) -> Self {
        self.proxy = Some(proxy.to_owned());
        self
    }

    fn poll(&self, url: &str, query: &[(&str, &str)]) -> Result<Vec<u8>, GifError> {
        self.request(url, query, JSON_LIMIT_BYTES, true)
    }

    fn download(&self, url: &str, max: u64) -> Result<Vec<u8>, GifError> {
        self.request(url, &[], max, false)
    }

    fn request(
        &self,
        url: &str,
        query: &[(&str, &str)],
        max: u64,
        is_api: bool,
    ) -> Result<Vec<u8>, GifError> {
        if !self.allowed(url, is_api) {
            return Err(GifError::DisallowedAddress);
        }
        let mut request = self.agent.get(url);
        for (name, value) in query {
            request = request.query(name, value);
        }
        let mut response = request.call().map_err(|_| GifError::Offline)?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 if is_api => return Err(GifError::InvalidKey),
            429 => return Err(GifError::Limit),
            state => return Err(GifError::Http { state }),
        }
        let declared = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if declared.is_some_and(|bytes| bytes > max) {
            return Err(GifError::TooLarge);
        }
        response
            .body_mut()
            .with_config()
            .limit(max)
            .read_to_vec()
            .map_err(|error| match error {
                ureq::Error::BodyExceedsLimit(_) => GifError::TooLarge,
                _ => GifError::Offline,
            })
    }
}

impl Http {
    fn allowed(&self, url: &str, is_api: bool) -> bool {
        // The proxy has no path, so this prefix pins the exact authority.
        let of_proxy = is_api
            && self.proxy.as_deref().is_some_and(|proxy| {
                url.strip_prefix(proxy)
                    .is_some_and(|rest| rest.starts_with('/'))
            });
        of_proxy || allowed_host(url, self.hosts)
    }
}

/// Validates the app's proxy address (`CONVERSOR_PROXY_URL`): `https` and host only, with no
/// credentials, path, query or fragment. Returns it without a trailing slash.
pub(crate) fn validate_proxy(url: &str) -> Result<String, GifError> {
    let url = url.trim().trim_end_matches('/');
    let authority = url
        .strip_prefix("https://")
        .or_else(|| {
            // Tests talk to a local HTTP server, as with `allowed_host`.
            url.strip_prefix("http://")
                .filter(|rest| cfg!(test) && rest.starts_with("127.0.0.1:"))
        })
        .ok_or(GifError::InvalidProxy)?;
    let valid = !authority.is_empty()
        && authority
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if valid {
        Ok(url.to_owned())
    } else {
        Err(GifError::InvalidProxy)
    }
}

fn allowed_host(url: &str, hosts: &[&str]) -> bool {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((schema, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    if cfg!(test) && schema == "http" && authority.starts_with("127.0.0.1:") {
        return true;
    }
    schema == "https" && hosts.contains(&authority)
}

fn validate_query(text: &str) -> Result<&str, GifError> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_QUERY_LENGTH {
        return Err(GifError::InvalidQuery);
    }
    Ok(text)
}

pub(crate) fn validate_key(key: &str) -> Result<(), GifError> {
    if key.is_empty() || key.len() > MAX_KEY_LENGTH || !key.chars().all(is_id_char) {
        return Err(GifError::InvalidKey);
    }
    Ok(())
}

fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn extension_of(url: &str) -> Option<&'static str> {
    let route = url.split(['?', '#']).next()?;
    let extension = route.rsplit_once('.')?.1.to_ascii_lowercase();
    EXTENSIONS.iter().copied().find(|e| *e == extension)
}

pub(crate) struct Klipy {
    http: Http,
    /// Everything before the route: with a key, `<base>/api/v1/<key>`; through the app's proxy,
    /// `<proxy>/klipy`, which holds the key itself.
    prefix: String,
}

impl Klipy {
    pub(crate) fn new(key: &str) -> Result<Self, GifError> {
        Self::with_base(KLIPY_URL, key)
    }

    pub(crate) fn through_proxy(proxy: &str) -> Result<Self, GifError> {
        let proxy = validate_proxy(proxy)?;
        Ok(Klipy {
            prefix: format!("{proxy}/klipy"),
            http: Http::new(KLIPY_HOSTS).with_proxy(&proxy),
        })
    }

    fn with_base(base: &str, key: &str) -> Result<Self, GifError> {
        validate_key(key)?;
        Ok(Klipy {
            http: Http::new(KLIPY_HOSTS),
            prefix: format!("{}/api/v1/{key}", base.trim_end_matches('/')),
        })
    }

    fn request(&self, route: &str, query: &[(&str, &str)]) -> Result<GifPage, GifError> {
        let page = self.poll(&format!("gifs/{route}"), query)?;
        parse_klipy(&page, self.http.hosts)
    }

    fn poll(&self, route: &str, query: &[(&str, &str)]) -> Result<Vec<u8>, GifError> {
        let url = format!("{}/{route}", self.prefix);
        self.http.poll(&url, query)
    }

    pub(crate) fn stickers(&self, text: Option<&str>, page: u32) -> Result<StickerPage, GifError> {
        let (page, per_page) = (page.max(1).to_string(), PER_PAGE.to_string());
        let mut query = vec![
            ("page", page.as_str()),
            ("per_page", per_page.as_str()),
            ("content_filter", "medium"),
        ];
        let route = match text {
            Some(text) => {
                query.push(("q", validate_query(text)?));
                "stickers/search"
            }
            None => "stickers/trending",
        };
        let body = self.poll(route, &query)?;
        parse_klipy_stickers(&body, self.http.hosts)
    }

    pub(crate) fn sticker_categories(&self) -> Result<Vec<KlipyCategory>, GifError> {
        let body = self.poll("stickers/categories", &[])?;
        parse_klipy_categories(&body)
    }
}

impl GifProvider for Klipy {
    fn name(&self) -> &'static str {
        "klipy"
    }

    fn attribution(&self) -> &'static str {
        "Powered by KLIPY"
    }

    fn find(&self, text: &str, page: u32) -> Result<GifPage, GifError> {
        let text = validate_query(text)?;
        let (page, per_page) = (page.max(1).to_string(), PER_PAGE.to_string());
        self.request(
            "search",
            &[
                ("q", text),
                ("page", &page),
                ("per_page", &per_page),
                ("content_filter", "medium"),
            ],
        )
    }

    fn trending(&self, page: u32) -> Result<GifPage, GifError> {
        let (page, per_page) = (page.max(1).to_string(), PER_PAGE.to_string());
        self.request(
            "trending",
            &[
                ("page", &page),
                ("per_page", &per_page),
                ("content_filter", "medium"),
            ],
        )
    }

    fn download(&self, url: &str, max: u64) -> Result<Vec<u8>, GifError> {
        self.http.download(url, max)
    }
}

#[derive(Deserialize)]
struct KlipyResponse {
    result: Option<bool>,
    data: KlipyData,
}

#[derive(Deserialize)]
struct KlipyData {
    data: Vec<KlipyItem>,
    current_page: u32,
    has_next: bool,
}

#[derive(Deserialize)]
struct KlipyItem {
    id: serde_json::Number,
    #[serde(default)]
    title: String,
    file: std::collections::HashMap<String, std::collections::HashMap<String, KlipyFormat>>,
}

#[derive(Deserialize)]
struct KlipyFormat {
    url: String,
    width: u32,
    height: u32,
}

impl KlipyItem {
    fn format(&self, size: &str, format: &str) -> Option<&KlipyFormat> {
        self.file.get(size)?.get(format)
    }
}

fn parse_klipy(bytes: &[u8], hosts: &[&str]) -> Result<GifPage, GifError> {
    let response: KlipyResponse =
        serde_json::from_slice(bytes).map_err(|_| GifError::InvalidResponse)?;
    if response.result == Some(false) {
        return Err(GifError::InvalidResponse);
    }
    let data = response.data;
    let results = data
        .data
        .iter()
        .filter_map(|item| {
            let thumbnail = [("sm", "webp"), ("sm", "gif"), ("xs", "webp"), ("xs", "gif")]
                .into_iter()
                .find_map(|(size, format)| item.format(size, format))?;
            let media = [
                ("md", "mp4"),
                ("sm", "mp4"),
                ("hd", "mp4"),
                ("md", "gif"),
                ("sm", "gif"),
            ]
            .into_iter()
            .find_map(|(size, format)| item.format(size, format))?;
            valid_result(
                item.id.to_string(),
                &item.title,
                (thumbnail.width, thumbnail.height),
                &thumbnail.url,
                &media.url,
                hosts,
            )
        })
        .collect();
    Ok(GifPage {
        results,
        next: data.has_next.then(|| data.current_page + 1),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StickerResult {
    pub(crate) result: GifResult,
    pub(crate) animated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StickerPage {
    pub(crate) results: Vec<StickerResult>,
    pub(crate) next: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KlipyCategory {
    pub(crate) id: String,
    pub(crate) name: String,
}

fn parse_klipy_stickers(bytes: &[u8], hosts: &[&str]) -> Result<StickerPage, GifError> {
    let response: KlipyResponse =
        serde_json::from_slice(bytes).map_err(|_| GifError::InvalidResponse)?;
    if response.result == Some(false) {
        return Err(GifError::InvalidResponse);
    }
    let data = response.data;
    let results = data
        .data
        .iter()
        .filter_map(|item| {
            let find = |options: &[(&str, &str)]| {
                options
                    .iter()
                    .find_map(|(size, format)| item.format(size, format))
            };
            let animated = item
                .file
                .values()
                .any(|formats| formats.contains_key("webm"));
            let thumbnail = find(&[
                ("sm", "webp"),
                ("sm", "gif"),
                ("sm", "png"),
                ("xs", "webp"),
                ("xs", "png"),
                ("xs", "gif"),
            ])?;
            let media = if animated {
                find(&[("md", "gif"), ("sm", "gif"), ("hd", "gif")])?
            } else {
                find(&[("md", "png"), ("sm", "png"), ("hd", "png")])?
            };
            let result = valid_result(
                item.id.to_string(),
                &item.title,
                (thumbnail.width, thumbnail.height),
                &thumbnail.url,
                &media.url,
                hosts,
            )?;
            Some(StickerResult { result, animated })
        })
        .collect();
    Ok(StickerPage {
        results,
        next: data.has_next.then(|| data.current_page + 1),
    })
}

#[derive(Deserialize)]
struct CategoriesResponse {
    data: CategoriesData,
}

#[derive(Deserialize)]
struct CategoriesData {
    categories: Vec<RawCategory>,
}

#[derive(Deserialize)]
struct RawCategory {
    category: String,
    query: String,
}

fn parse_klipy_categories(bytes: &[u8]) -> Result<Vec<KlipyCategory>, GifError> {
    let response: CategoriesResponse =
        serde_json::from_slice(bytes).map_err(|_| GifError::InvalidResponse)?;
    Ok(response
        .data
        .categories
        .into_iter()
        .filter(|category| validate_query(&category.query).is_ok())
        .map(|category| KlipyCategory {
            id: category.query.trim().to_owned(),
            name: category.category,
        })
        .collect())
}

pub(crate) struct Giphy {
    http: Http,
    base: String,
    key: String,
}

impl Giphy {
    pub(crate) fn new(key: &str) -> Result<Self, GifError> {
        Self::with_base(GIPHY_URL, key)
    }

    fn with_base(base: &str, key: &str) -> Result<Self, GifError> {
        validate_key(key)?;
        Ok(Giphy {
            http: Http::new(GIPHY_HOSTS),
            base: base.trim_end_matches('/').to_owned(),
            key: key.to_owned(),
        })
    }

    fn request(&self, route: &str, page: u32, text: Option<&str>) -> Result<GifPage, GifError> {
        let offset = (page.max(1) - 1).saturating_mul(PER_PAGE);
        if offset > GIPHY_MAX_OFFSET {
            return Ok(GifPage {
                results: Vec::new(),
                next: None,
            });
        }
        let (offset, limit) = (offset.to_string(), PER_PAGE.to_string());
        let mut query = vec![
            ("api_key", self.key.as_str()),
            ("limit", limit.as_str()),
            ("offset", offset.as_str()),
            ("rating", "pg-13"),
        ];
        if let Some(text) = text {
            query.push(("q", text));
        }
        let url = format!("{}/v1/gifs/{route}", self.base);
        let body = self.http.poll(&url, &query)?;
        parse_giphy(&body, self.http.hosts)
    }
}

impl GifProvider for Giphy {
    fn name(&self) -> &'static str {
        "giphy"
    }

    fn attribution(&self) -> &'static str {
        "Powered by GIPHY"
    }

    fn find(&self, text: &str, page: u32) -> Result<GifPage, GifError> {
        self.request("search", page, Some(validate_query(text)?))
    }

    fn trending(&self, page: u32) -> Result<GifPage, GifError> {
        self.request("trending", page, None)
    }

    fn download(&self, url: &str, max: u64) -> Result<Vec<u8>, GifError> {
        self.http.download(url, max)
    }
}

#[derive(Deserialize)]
struct GiphyResponse {
    data: Vec<GiphyItem>,
    pagination: GiphyPagination,
}

#[derive(Deserialize)]
struct GiphyPagination {
    total_count: u32,
    count: u32,
    offset: u32,
}

#[derive(Deserialize)]
struct GiphyItem {
    id: String,
    #[serde(default)]
    title: String,
    images: GiphyImages,
}

#[derive(Deserialize)]
struct GiphyImages {
    fixed_width: Option<GiphyImage>,
    fixed_width_small: Option<GiphyImage>,
    original: Option<GiphyImage>,
}

#[derive(Deserialize)]
struct GiphyImage {
    url: Option<String>,
    mp4: Option<String>,
    width: Option<String>,
    height: Option<String>,
}

fn parse_giphy(bytes: &[u8], hosts: &[&str]) -> Result<GifPage, GifError> {
    let response: GiphyResponse =
        serde_json::from_slice(bytes).map_err(|_| GifError::InvalidResponse)?;
    let results = response
        .data
        .iter()
        .filter_map(|item| {
            let images = &item.images;
            let thumbnail = [&images.fixed_width_small, &images.fixed_width]
                .into_iter()
                .flatten()
                .find(|image| image.url.is_some())?;
            let media = [&images.original, &images.fixed_width]
                .into_iter()
                .flatten()
                .find_map(|image| image.mp4.as_ref().or(image.url.as_ref()))?;
            let measure = |text: &Option<String>| {
                text.as_deref()
                    .and_then(|t| t.parse::<u32>().ok())
                    .unwrap_or(0)
            };
            valid_result(
                item.id.clone(),
                &item.title,
                (measure(&thumbnail.width), measure(&thumbnail.height)),
                thumbnail.url.as_deref()?,
                media,
                hosts,
            )
        })
        .collect();
    let pagination = response.pagination;
    let next_offset = pagination.offset + pagination.count;
    let has_more = pagination.count > 0
        && next_offset < pagination.total_count
        && next_offset <= GIPHY_MAX_OFFSET;
    Ok(GifPage {
        results,
        next: has_more.then(|| next_offset / PER_PAGE + 1),
    })
}

fn valid_result(
    id: String,
    title: &str,
    (width, height): (u32, u32),
    thumbnail_url: &str,
    media_url: &str,
    hosts: &[&str],
) -> Option<GifResult> {
    let valid = !id.is_empty()
        && id.len() <= MAX_KEY_LENGTH
        && id.chars().all(is_id_char)
        && allowed_host(thumbnail_url, hosts)
        && allowed_host(media_url, hosts)
        && extension_of(thumbnail_url).is_some()
        && extension_of(media_url).is_some();
    valid.then(|| GifResult {
        id,
        title: title.to_owned(),
        width,
        height,
        thumbnail_url: thumbnail_url.to_owned(),
        media_url: media_url.to_owned(),
    })
}

fn disk(action: &'static str) -> impl FnOnce(io::Error) -> GifError {
    move |source| GifError::Disk { action, source }
}

fn to_cache(
    provider: &dyn GifProvider,
    cache: &Path,
    id: &str,
    url: &str,
    limit: u64,
) -> Result<PathBuf, GifError> {
    let extension = extension_of(url).ok_or(GifError::DisallowedAddress)?;
    let path = cache.join(format!("{}-{id}.{extension}", provider.name()));
    if path.is_file() {
        return Ok(path);
    }
    let bytes = provider.download(url, limit)?;
    fs::create_dir_all(cache).map_err(disk("crear la caché de GIF"))?;
    write_atomic(&path, &bytes).map_err(disk("guardar el GIF en la caché"))?;
    Ok(path)
}

pub(crate) fn thumbnail_in_cache(
    provider: &dyn GifProvider,
    cache: &Path,
    result: &GifResult,
) -> Result<PathBuf, GifError> {
    to_cache(
        provider,
        &cache.join("thumbnails"),
        &result.id,
        &result.thumbnail_url,
        THUMBNAIL_LIMIT_BYTES,
    )
}

pub(crate) fn with_thumbnails(
    provider: &dyn GifProvider,
    cache: &Path,
    results: Vec<GifResult>,
) -> Vec<(GifResult, PathBuf)> {
    std::thread::scope(|scope| {
        let threads: Vec<_> = results
            .into_iter()
            .map(|result| {
                scope.spawn(move || match thumbnail_in_cache(provider, cache, &result) {
                    Ok(path) => Some((result, path)),
                    Err(error) => {
                        warn!(id = %result.id, %error, "se omite un GIF sin miniatura");
                        None
                    }
                })
            })
            .collect();
        threads
            .into_iter()
            .filter_map(|thread| thread.join().ok().flatten())
            .collect()
    })
}

pub(crate) fn media_in_cache(
    provider: &dyn GifProvider,
    cache: &Path,
    result: &GifResult,
) -> Result<PathBuf, GifError> {
    to_cache(
        provider,
        &cache.join("media"),
        &result.id,
        &result.media_url,
        MEDIA_LIMIT_BYTES,
    )
}

pub(crate) fn prune_cache(cache: &Path, age: Duration) -> Result<(), GifError> {
    for subfolder in [
        "thumbnails",
        "media",
        "stickers/thumbnails",
        "stickers/media",
    ] {
        let entries = match fs::read_dir(cache.join(subfolder)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(disk("leer la caché de GIF")(source)),
        };
        for entry in entries {
            let entry = entry.map_err(disk("leer la caché de GIF"))?;
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|date| SystemTime::now().duration_since(date).ok())
                .is_some_and(|elapsed| elapsed > age);
            if old {
                fs::remove_file(entry.path()).map_err(disk("borrar un GIF antiguo"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    use super::*;

    const KLIPY_SEARCH: &str = include_str!("../gif_examples/klipy.json");
    const GIPHY_SEARCH: &str = include_str!("../gif_examples/giphy.json");
    const KLIPY_STICKERS: &str = include_str!("../gif_examples/klipy_stickers.json");
    const KLIPY_CATEGORIES: &str = include_str!("../gif_examples/klipy_categories.json");

    type Path = (u16, Vec<u8>);

    struct Server {
        url: String,
        paths: Arc<Mutex<HashMap<String, Path>>>,
        requested: Arc<Mutex<Vec<String>>>,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Server {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let paths: Arc<Mutex<HashMap<String, Path>>> = Arc::default();
            let requested: Arc<Mutex<Vec<String>>> = Arc::default();
            let stop = Arc::new(AtomicBool::new(false));
            let (paths_thread, requested_thread, stop_thread) = (
                Arc::clone(&paths),
                Arc::clone(&requested),
                Arc::clone(&stop),
            );
            let thread = std::thread::spawn(move || {
                for connection in listener.incoming() {
                    if stop_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(connection) = connection {
                        serve(connection, &paths_thread, &requested_thread);
                    }
                }
            });
            Server {
                url,
                paths,
                requested,
                stop,
                thread: Some(thread),
            }
        }

        fn put(&self, route: &str, state: u16, body: impl Into<Vec<u8>>) {
            self.paths
                .lock()
                .unwrap()
                .insert(route.to_owned(), (state, body.into()));
        }

        fn requested(&self) -> Vec<String> {
            self.requested.lock().unwrap().clone()
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn serve(
        mut connection: TcpStream,
        paths: &Mutex<HashMap<String, Path>>,
        requested: &Mutex<Vec<String>>,
    ) {
        let mut reader = BufReader::new(connection.try_clone().unwrap());
        let mut first = String::new();
        if reader.read_line(&mut first).unwrap_or(0) == 0 {
            return;
        }
        let target = first.split_whitespace().nth(1).unwrap_or("/").to_owned();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
        }
        requested.lock().unwrap().push(target.clone());
        let route = target.split('?').next().unwrap_or("/");
        let (state, body) = paths
            .lock()
            .unwrap()
            .get(route)
            .cloned()
            .unwrap_or((404, Vec::new()));
        let header = format!(
            "HTTP/1.1 {state} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let _ = connection.write_all(header.as_bytes());
        let _ = connection.write_all(&body);
    }

    fn klipy(server: &Server) -> Klipy {
        Klipy::with_base(&server.url, "clave-de-prueba").unwrap()
    }

    fn giphy(server: &Server) -> Giphy {
        Giphy::with_base(&server.url, "clave-de-prueba").unwrap()
    }

    #[test]
    fn klipy_searches_with_key_in_path_and_reads_results() {
        let server = Server::new();
        server.put("/api/v1/clave-de-prueba/gifs/search", 200, KLIPY_SEARCH);

        let page = klipy(&server).find("  hola  ", 2).unwrap();

        let requested = server.requested().remove(0);
        for chunk in ["q=hola", "page=2", "per_page=24", "content_filter=medium"] {
            assert!(requested.contains(chunk), "{requested}");
        }
        assert_eq!(page.next, Some(3));
        assert_eq!(page.results.len(), 1);
        assert_eq!(
            page.results[0],
            GifResult {
                id: "8041071659142944".to_owned(),
                title: "Hello".to_owned(),
                width: 220,
                height: 220,
                thumbnail_url: "https://static.klipy.com/ii/935d/sm.webp".to_owned(),
                media_url: "https://static.klipy.com/ii/935d/md.mp4".to_owned(),
            }
        );
    }

    #[test]
    fn klipy_through_proxy_uses_its_path_and_sends_no_key() {
        let server = Server::new();
        server.put("/klipy/gifs/search", 200, KLIPY_SEARCH);
        server.put("/klipy/stickers/trending", 200, KLIPY_STICKERS);
        server.put("/klipy/stickers/categories", 200, KLIPY_CATEGORIES);
        let proxy = format!("{}/", server.url);
        let klipy = Klipy::through_proxy(&proxy).unwrap();

        assert_eq!(klipy.find("hola", 1).unwrap().results.len(), 1);
        assert!(!klipy.stickers(None, 1).unwrap().results.is_empty());
        assert!(!klipy.sticker_categories().unwrap().is_empty());
        let requested = server.requested();
        assert_eq!(requested.len(), 3);
        assert!(requested[0].starts_with("/klipy/gifs/search?q=hola"));
        assert!(requested[1].starts_with("/klipy/stickers/trending?"));
        assert_eq!(requested[2], "/klipy/stickers/categories");
        assert!(requested.iter().all(|r| !r.contains("/api/v1/")));
    }

    #[test]
    fn proxy_is_only_trusted_for_api_calls_not_for_media() {
        let server = Server::new();
        let klipy = Klipy::through_proxy(&server.url).unwrap();
        // In tests any 127.0.0.1 is allowed, so check the rule directly.
        let proxy = Http::new(KLIPY_HOSTS).with_proxy("https://proxy.example");
        assert!(proxy.allowed("https://proxy.example/klipy/gifs/search", true));
        assert!(!proxy.allowed("https://proxy.example/klipy/media.gif", false));
        assert!(!proxy.allowed("https://proxy.example.evil.com/klipy/x", true));
        assert!(!proxy.allowed("https://proxy.example@evil.com/klipy/x", true));
        assert!(!proxy.allowed("https://otro.example/klipy/x", true));
        assert!(matches!(
            klipy.download("https://evil.example/a.gif", 10),
            Err(GifError::DisallowedAddress)
        ));
    }

    #[test]
    fn proxy_address_must_be_https_with_only_a_host() {
        assert_eq!(
            validate_proxy(" https://proxy.example.workers.dev/ ").unwrap(),
            "https://proxy.example.workers.dev"
        );
        assert_eq!(
            validate_proxy("https://proxy.example:8443").unwrap(),
            "https://proxy.example:8443"
        );
        for url in [
            "",
            "http://proxy.example",
            "ftp://proxy.example",
            "proxy.example",
            "https://",
            "https://user@proxy.example",
            "https://proxy.example/ruta",
            "https://proxy.example?x=1",
            "https://proxy .example",
            "http://localhost:8787",
        ] {
            assert!(
                matches!(validate_proxy(url), Err(GifError::InvalidProxy)),
                "{url}"
            );
            assert!(matches!(
                Klipy::through_proxy(url),
                Err(GifError::InvalidProxy)
            ));
        }
    }

    #[test]
    fn klipy_trending_uses_its_route() {
        let server = Server::new();
        server.put(
            "/api/v1/clave-de-prueba/gifs/trending",
            200,
            KLIPY_SEARCH.replace("\"has_next\": true", "\"has_next\": false"),
        );
        let page = klipy(&server).trending(1).unwrap();
        assert_eq!(page.next, None);
        assert_eq!(page.results.len(), 1);
    }

    #[test]
    fn giphy_searches_with_rating_and_computes_next_page() {
        let server = Server::new();
        server.put("/v1/gifs/search", 200, GIPHY_SEARCH);

        let page = giphy(&server).find("gato", 2).unwrap();

        let requested = server.requested().remove(0);
        for chunk in [
            "api_key=clave-de-prueba",
            "q=gato",
            "offset=24",
            "limit=24",
            "rating=pg-13",
        ] {
            assert!(requested.contains(chunk), "{requested}");
        }
        assert_eq!(page.next, Some(3));
        assert_eq!(page.results.len(), 1);
        assert_eq!(
            page.results[0],
            GifResult {
                id: "xT9IgzoKnwFNmISR8I".to_owned(),
                title: "Cat GIF".to_owned(),
                width: 100,
                height: 80,
                thumbnail_url: "https://media1.giphy.com/media/xT9/100w.gif".to_owned(),
                media_url: "https://media1.giphy.com/media/xT9/giphy.mp4".to_owned(),
            }
        );
    }

    #[test]
    fn giphy_does_not_request_beyond_max_offset() {
        let server = Server::new();
        let page = giphy(&server).trending(100).unwrap();
        assert!(page.results.is_empty());
        assert_eq!(page.next, None);
        assert!(server.requested().is_empty());
    }

    #[test]
    fn rejected_key_limit_and_other_errors() {
        let server = Server::new();
        let provider = giphy(&server);
        for (state, expected) in [
            (401, "La clave no es válida"),
            (403, "La clave no es válida"),
            (429, "Límite de búsquedas: espera un minuto"),
            (500, "El servicio de GIF respondió con el error 500"),
        ] {
            server.put("/v1/gifs/trending", state, "");
            let error = provider.trending(1).unwrap_err();
            assert_eq!(error.to_string(), expected);
            assert!(!error.to_string().contains("clave-de-prueba"));
        }
    }

    #[test]
    fn messages_are_what_the_interface_expects() {
        for (error, text) in [
            (
                GifError::MissingKey { provider: "KLIPY" },
                "Falta la clave de KLIPY",
            ),
            (
                GifError::MissingKey { provider: "GIPHY" },
                "Falta la clave de GIPHY",
            ),
            (GifError::InvalidKey, "La clave no es válida"),
            (GifError::Limit, "Límite de búsquedas: espera un minuto"),
            (GifError::Offline, "Sin conexión"),
        ] {
            assert_eq!(error.to_string(), text);
        }
    }

    #[test]
    fn no_server_is_offline_and_error_has_no_key() {
        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", listener.local_addr().unwrap())
        };
        let error = Klipy::with_base(&closed, "clave-de-prueba")
            .unwrap()
            .trending(1)
            .unwrap_err();
        assert_eq!(error.to_string(), "Sin conexión");
    }

    #[test]
    fn huge_response_is_cut() {
        let server = Server::new();
        server.put(
            "/v1/gifs/trending",
            200,
            vec![b' '; JSON_LIMIT_BYTES as usize + 1],
        );
        assert!(matches!(
            giphy(&server).trending(1),
            Err(GifError::TooLarge)
        ));
    }

    #[test]
    fn response_not_in_format_is_invalid() {
        let server = Server::new();
        server.put("/v1/gifs/trending", 200, r#"{"data": 3}"#);
        assert!(matches!(
            giphy(&server).trending(1),
            Err(GifError::InvalidResponse)
        ));
    }

    #[test]
    fn results_with_host_outside_list_are_discarded() {
        let bad = KLIPY_SEARCH.replace("static.klipy.com", "evil.example");
        let page = parse_klipy(bad.as_bytes(), KLIPY_HOSTS).unwrap();
        assert!(page.results.is_empty());

        let bad = GIPHY_SEARCH.replace("media1.giphy.com", "media1.giphy.com.evil.example");
        let page = parse_giphy(bad.as_bytes(), GIPHY_HOSTS).unwrap();
        assert!(page.results.is_empty());
    }

    #[test]
    fn only_https_to_listed_hosts() {
        let hosts = &["static.klipy.com"];
        assert!(allowed_host("https://static.klipy.com/a.gif", hosts));
        assert!(!allowed_host("http://static.klipy.com/a.gif", hosts));
        assert!(!allowed_host(
            "https://static.klipy.com@evil.example/a",
            hosts
        ));
        assert!(!allowed_host("https://static.klipy.com:8443/a", hosts));
        assert!(!allowed_host("https://otro.klipy.com/a", hosts));
        assert!(!allowed_host("https://static.klipy.com/a b", hosts));
    }

    #[test]
    fn download_outside_list_makes_no_request() {
        let server = Server::new();
        let error = klipy(&server)
            .download("https://evil.example/a.gif", 1000)
            .unwrap_err();
        assert!(matches!(error, GifError::DisallowedAddress));
    }

    #[test]
    fn query_and_key_are_validated() {
        let server = Server::new();
        assert!(matches!(
            klipy(&server).find("   ", 1),
            Err(GifError::InvalidQuery)
        ));
        assert!(matches!(
            klipy(&server).find(&"a".repeat(101), 1),
            Err(GifError::InvalidQuery)
        ));
        for key in ["", "con/barra", "con espacio", "a?b=c"] {
            assert!(matches!(Klipy::new(key), Err(GifError::InvalidKey)));
        }
    }

    #[test]
    fn thumbnail_and_media_are_cached_with_their_limits() {
        let server = Server::new();
        server.put("/mini.gif", 200, b"GIF89a-mini".to_vec());
        server.put(
            "/grande.gif",
            200,
            vec![0; THUMBNAIL_LIMIT_BYTES as usize + 1],
        );
        server.put("/medio.mp4", 200, b"video".to_vec());
        let cache = tempfile::tempdir().unwrap();
        let provider = giphy(&server);
        let result = |id: &str, thumbnail: &str, media: &str| GifResult {
            id: id.to_owned(),
            title: String::new(),
            width: 1,
            height: 1,
            thumbnail_url: format!("{}{thumbnail}", server.url),
            media_url: format!("{}{media}", server.url),
        };

        let good = result("abc", "/mini.gif", "/medio.mp4");
        let path = thumbnail_in_cache(&provider, cache.path(), &good).unwrap();
        assert_eq!(path, cache.path().join("thumbnails/giphy-abc.gif"));
        assert_eq!(fs::read(&path).unwrap(), b"GIF89a-mini");
        let before = server.requested().len();
        thumbnail_in_cache(&provider, cache.path(), &good).unwrap();
        assert_eq!(server.requested().len(), before);

        let media = media_in_cache(&provider, cache.path(), &good).unwrap();
        assert_eq!(fs::read(media).unwrap(), b"video");

        let large = result("zzz", "/grande.gif", "/medio.mp4");
        assert!(matches!(
            thumbnail_in_cache(&provider, cache.path(), &large),
            Err(GifError::TooLarge)
        ));
        assert!(!cache.path().join("thumbnails/giphy-zzz.gif").exists());
    }

    #[test]
    fn failing_thumbnail_skips_only_that_result() {
        let server = Server::new();
        server.put("/bien.gif", 200, b"GIF89a".to_vec());
        let cache = tempfile::tempdir().unwrap();
        let result = |id: &str, thumbnail: &str| GifResult {
            id: id.to_owned(),
            title: String::new(),
            width: 1,
            height: 1,
            thumbnail_url: format!("{}{thumbnail}", server.url),
            media_url: format!("{}/medio.mp4", server.url),
        };

        let done = with_thumbnails(
            &giphy(&server),
            cache.path(),
            vec![result("uno", "/bien.gif"), result("dos", "/falta.gif")],
        );

        assert_eq!(done.len(), 1);
        assert_eq!(done[0].0.id, "uno");
        assert!(done[0].1.is_file());
    }

    #[test]
    fn pruning_deletes_only_old_files() {
        let cache = tempfile::tempdir().unwrap();
        let folder = cache.path().join("thumbnails");
        fs::create_dir_all(&folder).unwrap();
        let old = folder.join("vieja.gif");
        let new = folder.join("nueva.gif");
        fs::write(&old, b"x").unwrap();
        fs::write(&new, b"x").unwrap();
        let two_days_ago = SystemTime::now() - Duration::from_secs(48 * 3600);
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(two_days_ago)
            .unwrap();

        prune_cache(cache.path(), Duration::from_secs(24 * 3600)).unwrap();

        assert!(!old.exists());
        assert!(new.exists());
        prune_cache(&cache.path().join("no-existe"), Duration::ZERO).unwrap();
    }

    #[test]
    fn klipy_stickers_trending_and_search_read_animated_and_fixed() {
        let server = Server::new();
        server.put(
            "/api/v1/clave-de-prueba/stickers/trending",
            200,
            KLIPY_STICKERS,
        );
        server.put(
            "/api/v1/clave-de-prueba/stickers/search",
            200,
            KLIPY_STICKERS.replace("\"has_next\": true", "\"has_next\": false"),
        );

        let trending = klipy(&server).stickers(None, 1).unwrap();
        assert_eq!(trending.next, Some(2));
        assert_eq!(trending.results.len(), 2);
        let animated = &trending.results[0];
        assert!(animated.animated);
        assert_eq!(animated.result.id, "6914457558367152");
        assert_eq!(
            animated.result.thumbnail_url,
            "https://static.klipy.com/ii/ffd4/sm.webp"
        );
        assert_eq!(
            animated.result.media_url,
            "https://static.klipy.com/ii/ffd4/md.gif"
        );
        let fixed = &trending.results[1];
        assert!(!fixed.animated);
        assert_eq!(
            fixed.result.media_url,
            "https://static.klipy.com/ii/aa/md.png"
        );

        let search = klipy(&server).stickers(Some("  oso "), 3).unwrap();
        assert_eq!(search.next, None);
        let requested = server.requested().remove(1);
        for chunk in ["q=oso", "page=3", "per_page=24", "content_filter=medium"] {
            assert!(requested.contains(chunk), "{requested}");
        }
        assert!(klipy(&server).stickers(Some("  "), 1).is_err());
    }

    #[test]
    fn klipy_sticker_categories_discards_empty_ones() {
        let server = Server::new();
        server.put(
            "/api/v1/clave-de-prueba/stickers/categories",
            200,
            KLIPY_CATEGORIES,
        );
        let categories = klipy(&server).sticker_categories().unwrap();
        assert_eq!(
            categories,
            vec![
                KlipyCategory {
                    id: "smile".to_owned(),
                    name: "smile".to_owned()
                },
                KlipyCategory {
                    id: "me gusta".to_owned(),
                    name: "Me gusta".to_owned()
                },
            ]
        );
    }

    #[test]
    fn stickers_share_key_and_limit_errors() {
        let server = Server::new();
        server.put("/api/v1/clave-de-prueba/stickers/trending", 401, "");
        server.put("/api/v1/clave-de-prueba/stickers/categories", 429, "");
        assert!(matches!(
            klipy(&server).stickers(None, 1),
            Err(GifError::InvalidKey)
        ));
        assert!(matches!(
            klipy(&server).sticker_categories(),
            Err(GifError::Limit)
        ));
    }

    #[test]
    fn pruning_reaches_sticker_cache() {
        let cache = tempfile::tempdir().unwrap();
        let folder = cache.path().join("stickers/media");
        fs::create_dir_all(&folder).unwrap();
        let old = folder.join("vieja.gif");
        fs::write(&old, b"x").unwrap();
        prune_cache(cache.path(), Duration::ZERO).unwrap();
        assert!(!old.exists());
    }
}
