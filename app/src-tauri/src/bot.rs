//! Client of the control API (version 1) of the user's own sticker bot, and the saved connection to
//! it. The bot is reached through Tailscale or on this machine; everything it answers is untrusted.
//! Nothing here logs the token, the pairing code, the QR or phone numbers.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::contract::{UiError, UiErrorKind};
use crate::contract_bot::{BotConnectionUi, BotLink, BotStatusUi, LinkMethod, LinkState};

const API_VERSION: u32 = 1;
const CONNECT_TIME_LIMIT: Duration = Duration::from_secs(5);
const TIME_LIMIT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: u64 = 8 * 1024;
const MAX_CONNECTION_FILE_BYTES: u64 = 4 * 1024;
const MAX_URL_LENGTH: usize = 260;
const MAX_HOST_LENGTH: usize = 253;
/// 32 random bytes in unpadded base64url.
const TOKEN_LENGTH: usize = 43;
const MAX_BOT_TEXT_CHARS: usize = 300;
const MAX_QR_LENGTH: usize = 1024;
const CONNECTION_LINK_PREFIX: &str = "conversorstickers://bot#";
/// Address (at most 260 characters, up to three times that percent-encoded) plus the token.
const MAX_CONNECTION_LINK_LENGTH: usize = 1024;
const CONNECTION_FILE: &str = "bot-connection.json";
/// Where the bot leaves its token and port for apps on the same computer.
const LOCAL_BOT_FOLDER: &str = "bot-stickers-sonido";
const LOCAL_TOKEN_FILE: &str = "control-token";
const LOCAL_PORT_FILE: &str = "conexion.json";
const MAX_LOCAL_FILE_BYTES: u64 = 4 * 1024;

#[derive(Debug, thiserror::Error)]
pub(crate) enum BotError {
    #[error(
        "la dirección no vale: usa https://equipo.tu-red.ts.net (sin ruta ni usuario) o, para un bot en este mismo equipo, http://127.0.0.1:8787"
    )]
    InvalidUrl,

    #[error(
        "el token no vale: copia entero el contenido del archivo control-token del bot ({TOKEN_LENGTH} caracteres)"
    )]
    InvalidToken,

    #[error(
        "el enlace de conexión no vale: copia entero el enlace que muestra el bot (empieza por conversorstickers://bot#)"
    )]
    InvalidConnectionLink,

    #[error("escribe el token: al cambiar la dirección hay que volver a pegarlo")]
    TokenRequired,

    #[error(
        "los archivos del bot en este equipo no valen; usa el enlace que muestra el bot al arrancar"
    )]
    InvalidLocalBot,

    #[error("la conexión con el bot no está habilitada")]
    NotEnabled,

    #[error("no se pudo {action} la conexión guardada con el bot: {source}")]
    Disk {
        action: &'static str,
        #[source]
        source: io::Error,
    },

    #[error(
        "la conexión guardada con el bot no se entiende; vuelve a poner la dirección y el token"
    )]
    CorruptConnection,

    #[error(
        "no se puede llegar al bot. ¿Está Tailscale activo en este equipo y en el del bot? ¿Está el bot arrancado?"
    )]
    Unreachable,

    #[error("Ya hay un bot conectado: pulsa Deshabilitar en «Mi bot» antes de usar otro enlace")]
    AlreadyConnected,

    #[error("el bot rechazó el token")]
    Unauthorized,

    #[error(
        "el bot ha bloqueado las peticiones por demasiados intentos fallidos; espera un minuto"
    )]
    TooManyAttempts,

    /// An error the bot explains itself (409, 422...).
    #[error("el bot responde: {0}")]
    Refused(String),

    #[error("respuesta inesperada del bot (código HTTP {0}); revisa la dirección")]
    UnexpectedStatus(u16),

    #[error("la respuesta del bot es demasiado grande; revisa la dirección")]
    TooLarge,

    #[error("la respuesta del bot no se entiende; revisa la dirección")]
    InvalidResponse,

    #[error(
        "el bot usa la versión {0} de su API y esta app entiende la {API_VERSION}; actualiza la app o el bot"
    )]
    IncompatibleVersion(u64),
}

impl From<BotError> for UiError {
    fn from(error: BotError) -> Self {
        let kind = match error {
            BotError::InvalidUrl
            | BotError::InvalidToken
            | BotError::InvalidConnectionLink
            | BotError::InvalidLocalBot
            | BotError::TokenRequired
            | BotError::AlreadyConnected => UiErrorKind::InvalidEntry,
            BotError::Unreachable => UiErrorKind::BotUnreachable,
            BotError::Unauthorized => UiErrorKind::BotUnauthorized,
            _ => UiErrorKind::Other,
        };
        UiError::new(kind, error.to_string())
    }
}

/// The base address of the bot, normalized: `https://<host>.ts.net[:port]`, or `http` to this
/// machine or to a private IPv4 of the local network. No user info, path, query or fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotUrl(String);

impl BotUrl {
    pub(crate) fn parse(text: &str) -> Result<Self, BotError> {
        let text = text.trim();
        if text.len() > MAX_URL_LENGTH || !text.is_ascii() {
            return Err(BotError::InvalidUrl);
        }
        let text = text.to_ascii_lowercase();
        let (scheme, rest) = text.split_once("://").ok_or(BotError::InvalidUrl)?;
        let authority = rest.strip_suffix('/').unwrap_or(rest);
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        if port.is_some_and(|port| !valid_port(port)) {
            return Err(BotError::InvalidUrl);
        }
        let allowed = match scheme {
            "https" => tailnet_host(host),
            "http" => host == "127.0.0.1" || host == "localhost" || private_ipv4(host),
            _ => false,
        };
        if !allowed {
            return Err(BotError::InvalidUrl);
        }
        Ok(BotUrl(format!("{scheme}://{authority}")))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_port(port: &str) -> bool {
    port.bytes().all(|byte| byte.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|number| number > 0)
}

/// Four plain decimal octets (no leading zeros, so no octal reading) in 10/8, 172.16/12 or
/// 192.168/16. Names, IPv6 and whole-number forms never match.
fn private_ipv4(host: &str) -> bool {
    let octets: Vec<u8> = host
        .split('.')
        .map(|part| {
            let plain = !part.is_empty()
                && part.len() <= 3
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'));
            if plain { part.parse().ok() } else { None }
        })
        .collect::<Option<_>>()
        .unwrap_or_default();
    matches!(
        octets[..],
        [10, _, _, _] | [172, 16..=31, _, _] | [192, 168, _, _]
    )
}

/// Only letters, digits and hyphens in each label, so nothing like `@`, `/` or `%` gets through.
fn tailnet_host(host: &str) -> bool {
    host.len() <= MAX_HOST_LENGTH
        && host.strip_suffix(".ts.net").is_some_and(|name| {
            name.split('.').all(|label| {
                !label.is_empty()
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        })
}

/// The bot's secret. No `Debug` nor `Display`, so it cannot be logged by accident.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BotToken(String);

impl BotToken {
    pub(crate) fn parse(text: &str) -> Result<Self, BotError> {
        let text = text.trim();
        let valid = text.len() == TOKEN_LENGTH
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if valid {
            Ok(BotToken(text.to_owned()))
        } else {
            Err(BotError::InvalidToken)
        }
    }
}

/// What a connection link carries, already validated. No `Debug`: it holds the token.
pub(crate) struct ConnectionLink {
    url: BotUrl,
    token: BotToken,
}

impl ConnectionLink {
    /// `conversorstickers://bot#u=<address, encodeURIComponent>&t=<token>`: exactly those two
    /// values, each checked with the same rules as when typed. Errors never repeat the link.
    pub(crate) fn parse(link: &str) -> Result<Self, BotError> {
        let link = link.trim();
        if link.len() > MAX_CONNECTION_LINK_LENGTH || !link.is_ascii() {
            return Err(BotError::InvalidConnectionLink);
        }
        let parameters = link
            .strip_prefix(CONNECTION_LINK_PREFIX)
            .ok_or(BotError::InvalidConnectionLink)?;
        let (mut url, mut token) = (None, None);
        for parameter in parameters.split('&') {
            let (name, value) = parameter
                .split_once('=')
                .ok_or(BotError::InvalidConnectionLink)?;
            let slot = match name {
                "u" => &mut url,
                "t" => &mut token,
                _ => return Err(BotError::InvalidConnectionLink),
            };
            if slot.replace(value).is_some() {
                return Err(BotError::InvalidConnectionLink);
            }
        }
        let (Some(url), Some(token)) = (url, token) else {
            return Err(BotError::InvalidConnectionLink);
        };
        let url = percent_decoded(url).ok_or(BotError::InvalidConnectionLink)?;
        Ok(ConnectionLink {
            url: BotUrl::parse(&url)?,
            token: BotToken::parse(token)?,
        })
    }

    pub(crate) fn url(&self) -> &str {
        self.url.as_str()
    }
}

/// Reverses `encodeURIComponent`; `None` for a broken escape or bytes that are not ASCII.
fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let digits = text.get(at + 1..at + 3)?;
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    decoded
        .is_ascii()
        .then(|| String::from_utf8(decoded).ok())?
}

/// The link another app opened, waiting for the person to confirm it. Only its address ever
/// reaches the interface; the token stays here until it is saved or dropped.
#[derive(Default)]
pub(crate) struct PendingLink(Mutex<Option<ConnectionLink>>);

impl PendingLink {
    fn slot(&self) -> MutexGuard<'_, Option<ConnectionLink>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces any earlier one and returns the address to ask about.
    pub(crate) fn offer(&self, link: ConnectionLink) -> String {
        let address = link.url().to_owned();
        *self.slot() = Some(link);
        address
    }

    pub(crate) fn take(&self) -> Option<ConnectionLink> {
        self.slot().take()
    }
}

#[derive(Deserialize)]
struct LocalPort {
    version: u32,
    puerto: u16,
}

/// The folder where the bot keeps its token: `$XDG_CONFIG_HOME` if it is an absolute path, else
/// `~/.config`. `None` on Android and Windows, where the bot is never on the same computer.
pub(crate) fn local_bot_folder() -> Option<PathBuf> {
    if !cfg!(unix) || cfg!(target_os = "android") {
        return None;
    }
    bot_folder_in(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn bot_folder_in(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let absolute =
        |value: Option<OsString>| value.map(PathBuf::from).filter(|path| path.is_absolute());
    let config =
        absolute(xdg_config_home).or_else(|| absolute(home).map(|home| home.join(".config")))?;
    Some(config.join(LOCAL_BOT_FOLDER))
}

fn read_small(path: &Path) -> Result<Option<Vec<u8>>, BotError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(BotError::Disk {
                action: "leer",
                source: error,
            });
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_LOCAL_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| BotError::Disk {
            action: "leer",
            source,
        })?;
    if bytes.len() as u64 > MAX_LOCAL_FILE_BYTES {
        return Err(BotError::InvalidLocalBot);
    }
    Ok(Some(bytes))
}

/// The bot of this computer, from its token and `conexion.json` in `folder`. `None` if either file
/// is missing; invalid contents fail without quoting them (the token is in there).
fn read_local_bot(folder: &Path) -> Result<Option<(BotUrl, BotToken)>, BotError> {
    let (Some(token), Some(port)) = (
        read_small(&folder.join(LOCAL_TOKEN_FILE))?,
        read_small(&folder.join(LOCAL_PORT_FILE))?,
    ) else {
        return Ok(None);
    };
    let token = std::str::from_utf8(&token).map_err(|_| BotError::InvalidLocalBot)?;
    let token = BotToken::parse(token).map_err(|_| BotError::InvalidLocalBot)?;
    let port: LocalPort = serde_json::from_slice(&port).map_err(|_| BotError::InvalidLocalBot)?;
    if port.version != 1 || port.puerto == 0 {
        return Err(BotError::InvalidLocalBot);
    }
    let url = BotUrl::parse(&format!("http://127.0.0.1:{}", port.puerto))?;
    Ok(Some((url, token)))
}

struct Connection {
    url: BotUrl,
    token: BotToken,
    enabled: bool,
}

impl Connection {
    fn summary(&self) -> BotConnectionUi {
        BotConnectionUi {
            url: Some(self.url.as_str().to_owned()),
            has_token: true,
            enabled: self.enabled,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SavedConnection {
    url: String,
    token: String,
    enabled: bool,
}

/// Where the connection is kept. On Android it goes under `no_backup/` of the app's data folder
/// (`getNoBackupFilesDir()`), which Android leaves out of cloud backups and device transfers. The
/// app's own backup only takes the library folder.
pub(crate) fn connection_file(app_data: &Path) -> PathBuf {
    if cfg!(target_os = "android") {
        app_data.join("no_backup").join(CONNECTION_FILE)
    } else {
        app_data.join(CONNECTION_FILE)
    }
}

pub(crate) struct ConnectionStore {
    path: PathBuf,
    /// Serializes read-modify-write between concurrent commands.
    lock: Mutex<()>,
}

impl ConnectionStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        ConnectionStore {
            path,
            lock: Mutex::default(),
        }
    }

    fn guard(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn summary(&self) -> Result<BotConnectionUi, BotError> {
        let _guard = self.guard();
        Ok(self
            .read()?
            .map(|connection| connection.summary())
            .unwrap_or_default())
    }

    /// Saves and enables the connection. Without a token the saved one is kept, but only for the
    /// same address: a token is never sent anywhere it was not typed for.
    pub(crate) fn connect(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> Result<BotConnectionUi, BotError> {
        let url = BotUrl::parse(url)?;
        let token = token.filter(|token| !token.trim().is_empty());
        let _guard = self.guard();
        let token = match token {
            Some(token) => BotToken::parse(token)?,
            None => match self.read()? {
                Some(saved) if saved.url == url => saved.token,
                _ => return Err(BotError::TokenRequired),
            },
        };
        self.save_enabled(url, token)
    }

    /// Saves and enables the connection a link carried, replacing the saved one.
    pub(crate) fn connect_link(&self, link: ConnectionLink) -> Result<BotConnectionUi, BotError> {
        let _guard = self.guard();
        self.save_enabled(link.url, link.token)
    }

    /// Connects to the bot of this computer from its own files, only if nothing is saved yet.
    /// `None` when something is saved or the bot left nothing in `folder`.
    pub(crate) fn connect_local(&self, folder: &Path) -> Result<Option<BotConnectionUi>, BotError> {
        let _guard = self.guard();
        if self.read()?.is_some() {
            return Ok(None);
        }
        let Some((url, token)) = read_local_bot(folder)? else {
            return Ok(None);
        };
        self.save_enabled(url, token).map(Some)
    }

    /// Fails while a connection is enabled: a link from another app never replaces it.
    pub(crate) fn ensure_not_enabled(&self) -> Result<(), BotError> {
        let _guard = self.guard();
        self.refuse_if_enabled()
    }

    /// Like `connect_link`, for a link another app opened: it never replaces an enabled one.
    pub(crate) fn connect_received_link(
        &self,
        link: ConnectionLink,
    ) -> Result<BotConnectionUi, BotError> {
        let _guard = self.guard();
        self.refuse_if_enabled()?;
        self.save_enabled(link.url, link.token)
    }

    fn refuse_if_enabled(&self) -> Result<(), BotError> {
        match self.read()? {
            Some(connection) if connection.enabled => Err(BotError::AlreadyConnected),
            _ => Ok(()),
        }
    }

    fn save_enabled(&self, url: BotUrl, token: BotToken) -> Result<BotConnectionUi, BotError> {
        let connection = Connection {
            url,
            token,
            enabled: true,
        };
        self.write(&connection)?;
        Ok(connection.summary())
    }

    /// Only the app stops asking: nothing is sent to the bot.
    pub(crate) fn disable(&self) -> Result<BotConnectionUi, BotError> {
        let _guard = self.guard();
        let Some(mut connection) = self.read()? else {
            return Ok(BotConnectionUi::default());
        };
        connection.enabled = false;
        self.write(&connection)?;
        Ok(connection.summary())
    }

    /// A client for the saved connection, only while it is enabled.
    pub(crate) fn client(&self) -> Result<BotClient, BotError> {
        let _guard = self.guard();
        match self.read()? {
            Some(connection) if connection.enabled => {
                Ok(BotClient::new(connection.url, connection.token))
            }
            _ => Err(BotError::NotEnabled),
        }
    }

    fn read(&self) -> Result<Option<Connection>, BotError> {
        let read_failed = |source| BotError::Disk {
            action: "leer",
            source,
        };
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(read_failed(error)),
        };
        let mut bytes = Vec::new();
        file.take(MAX_CONNECTION_FILE_BYTES)
            .read_to_end(&mut bytes)
            .map_err(read_failed)?;
        // The parse error is dropped on purpose: it can quote the file, token included.
        let saved: SavedConnection =
            serde_json::from_slice(&bytes).map_err(|_| BotError::CorruptConnection)?;
        match (BotUrl::parse(&saved.url), BotToken::parse(&saved.token)) {
            (Ok(url), Ok(token)) => Ok(Some(Connection {
                url,
                token,
                enabled: saved.enabled,
            })),
            _ => Err(BotError::CorruptConnection),
        }
    }

    /// Atomic. The temp file is created with mode 0600 on Unix and keeps it after the rename.
    fn write(&self, connection: &Connection) -> Result<(), BotError> {
        let saved = SavedConnection {
            url: connection.url.as_str().to_owned(),
            token: connection.token.0.clone(),
            enabled: connection.enabled,
        };
        let failed = |source| BotError::Disk {
            action: "guardar",
            source,
        };
        let directory = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(directory).map_err(failed)?;
        let bytes = serde_json::to_vec(&saved).map_err(|error| failed(error.into()))?;
        let mut temp = NamedTempFile::new_in(directory).map_err(failed)?;
        temp.write_all(&bytes).map_err(failed)?;
        temp.as_file().sync_all().map_err(failed)?;
        temp.persist(&self.path)
            .map_err(|error| failed(error.error))?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct WireVersion {
    version: u64,
}

#[derive(Deserialize)]
struct WireStatus {
    vinculacion: LinkState,
    conexion: BotLink,
    activo: bool,
    codigo: Option<String>,
    qr: Option<String>,
    aviso: Option<String>,
}

#[derive(Deserialize)]
struct WireError {
    error: String,
}

#[derive(Serialize)]
struct LinkRequest<'a> {
    metodo: LinkMethod,
    #[serde(skip_serializing_if = "Option::is_none")]
    numero: Option<&'a str>,
}

pub(crate) struct BotClient {
    url: BotUrl,
    token: BotToken,
    agent: ureq::Agent,
}

type Answer = Result<ureq::http::Response<ureq::Body>, ureq::Error>;

impl BotClient {
    fn new(url: BotUrl, token: BotToken) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIME_LIMIT))
            .timeout_global(Some(TIME_LIMIT))
            // A redirect could take the token somewhere else, and so could a proxy from the
            // environment (on plain http to this machine it would read it).
            .max_redirects(0)
            .proxy(None)
            .http_status_as_error(false)
            .build()
            .into();
        BotClient { url, token, agent }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{path}", self.url.as_str())
    }

    fn authorization(&self) -> String {
        format!("Bearer {}", self.token.0)
    }

    pub(crate) fn status(&self) -> Result<BotStatusUi, BotError> {
        read_status(
            self.agent
                .get(self.endpoint("estado"))
                .header("Authorization", self.authorization())
                .call(),
        )
    }

    pub(crate) fn activate(&self) -> Result<BotStatusUi, BotError> {
        self.post_empty("activar")
    }

    pub(crate) fn pause(&self) -> Result<BotStatusUi, BotError> {
        self.post_empty("pausar")
    }

    fn post_empty(&self, path: &str) -> Result<BotStatusUi, BotError> {
        read_status(
            self.agent
                .post(self.endpoint(path))
                .header("Authorization", self.authorization())
                .send_empty(),
        )
    }

    /// `phone` carries the person's number: it never goes into a message or a log.
    pub(crate) fn link(
        &self,
        method: LinkMethod,
        phone: Option<&str>,
    ) -> Result<BotStatusUi, BotError> {
        let request = LinkRequest {
            metodo: method,
            numero: phone,
        };
        let body = serde_json::to_string(&request).map_err(|_| BotError::InvalidResponse)?;
        read_status(
            self.agent
                .post(self.endpoint("vinculacion"))
                .header("Authorization", self.authorization())
                .header("Content-Type", "application/json")
                .send(body),
        )
    }

    pub(crate) fn unlink(&self) -> Result<BotStatusUi, BotError> {
        read_status(
            self.agent
                .delete(self.endpoint("vinculacion"))
                .header("Authorization", self.authorization())
                .call(),
        )
    }
}

fn read_status(answer: Answer) -> Result<BotStatusUi, BotError> {
    let mut response = answer.map_err(|_| BotError::Unreachable)?;
    let status = response.status().as_u16();
    match status {
        401 => return Err(BotError::Unauthorized),
        429 => return Err(BotError::TooManyAttempts),
        _ => {}
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => BotError::TooLarge,
            _ => BotError::Unreachable,
        })?;
    match status {
        200 | 202 => parse_status(&body),
        400..=599 => Err(serde_json::from_slice::<WireError>(&body)
            .ok()
            .and_then(|wire| bot_text(&wire.error))
            .map_or(BotError::UnexpectedStatus(status), BotError::Refused)),
        _ => Err(BotError::UnexpectedStatus(status)),
    }
}

fn parse_status(body: &[u8]) -> Result<BotStatusUi, BotError> {
    // The version is read first: another version may change the shape of the rest.
    let WireVersion { version } =
        serde_json::from_slice(body).map_err(|_| BotError::InvalidResponse)?;
    if version != u64::from(API_VERSION) {
        return Err(BotError::IncompatibleVersion(version));
    }
    let wire: WireStatus = serde_json::from_slice(body).map_err(|_| BotError::InvalidResponse)?;
    let linking = wire.vinculacion == LinkState::Linking;
    let code = wire.codigo.filter(|_| linking);
    let qr = wire.qr.filter(|_| linking);
    if code.as_deref().is_some_and(|code| !valid_code(code))
        || qr.as_deref().is_some_and(|qr| !valid_qr(qr))
    {
        return Err(BotError::InvalidResponse);
    }
    Ok(BotStatusUi {
        link: wire.vinculacion,
        connection: wire.conexion,
        active: wire.activo,
        code,
        qr,
        notice: wire.aviso.as_deref().and_then(bot_text),
    })
}

/// `ABCD-EFGH`.
fn valid_code(code: &str) -> bool {
    code.len() == 9
        && code.bytes().enumerate().all(|(index, byte)| {
            if index == 4 {
                byte == b'-'
            } else {
                byte.is_ascii_uppercase() || byte.is_ascii_digit()
            }
        })
}

fn valid_qr(qr: &str) -> bool {
    !qr.is_empty() && qr.len() <= MAX_QR_LENGTH && qr.bytes().all(|byte| byte.is_ascii_graphic())
}

/// A message written by the bot, on one line and cut short; `None` if nothing is left.
fn bot_text(text: &str) -> Option<String> {
    let clean: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_BOT_TEXT_CHARS)
        .collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    use super::*;

    const TOKEN: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDE_-";
    const OTHER_TOKEN: &str = "ZYXWVUTSRQPONMLKJIHGFEDCBA9876543210abcde-_";
    const PAUSED: &str = r#"{"version":1,"vinculacion":"vinculado","conexion":"conectado","activo":false,"codigo":null,"qr":null,"aviso":null}"#;

    /// Serves one request with `response` and hands back what it received.
    fn fake_bot(response: String) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let served = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                request.push_str(&line);
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            request.push_str(&String::from_utf8(body).unwrap());
            // The client may hang up before reading an answer it rejects.
            let _ = reader.get_mut().write_all(response.as_bytes());
            request
        });
        (url, served)
    }

    fn http(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn client(url: &str) -> BotClient {
        BotClient::new(BotUrl::parse(url).unwrap(), BotToken::parse(TOKEN).unwrap())
    }

    fn failure(result: Result<BotStatusUi, BotError>) -> BotError {
        match result {
            Ok(_) => panic!("an error was expected"),
            Err(error) => error,
        }
    }

    fn answer(status: &str, body: &str) -> Result<BotStatusUi, BotError> {
        let (url, served) = fake_bot(http(status, body));
        let result = client(&url).status();
        served.join().unwrap();
        result
    }

    #[test]
    fn accepts_tailnet_https_and_local_http() {
        for (typed, normalized) in [
            ("https://equipo.red-1.ts.net", "https://equipo.red-1.ts.net"),
            (" HTTPS://Equipo.Red.TS.net/ ", "https://equipo.red.ts.net"),
            (
                "https://equipo.red.ts.net:8443",
                "https://equipo.red.ts.net:8443",
            ),
            ("http://127.0.0.1:8787", "http://127.0.0.1:8787"),
            ("http://localhost:8787/", "http://localhost:8787"),
            ("http://192.168.1.10:8787", "http://192.168.1.10:8787"),
            ("http://10.0.0.5", "http://10.0.0.5"),
            ("http://172.16.0.1:9/", "http://172.16.0.1:9"),
            ("http://172.31.255.254", "http://172.31.255.254"),
        ] {
            assert_eq!(
                BotUrl::parse(typed).unwrap().as_str(),
                normalized,
                "{typed}"
            );
        }
    }

    #[test]
    fn rejects_any_other_address() {
        for typed in [
            "",
            "equipo.red.ts.net",
            "http://equipo.red.ts.net",
            "https://example.com",
            "https://ts.net",
            "https://.ts.net",
            "https://equipo..ts.net",
            "https://equipo.ts.net.example.com",
            "https://equipots.net",
            "https://user@equipo.red.ts.net",
            "https://user:pass@equipo.red.ts.net",
            "https://equipo.red.ts.net@example.com",
            "https://example.com/.ts.net",
            "https://example.com#.ts.net",
            "https://example.com?.ts.net",
            "https://example.com\\.ts.net",
            "https://equipo.red.ts.net/ruta",
            "https://equipo.red.ts.net//",
            "https://equipo.red.ts.net?a=1",
            "https://equipo.red.ts.net#a",
            "https://equipo.red.ts.net:0",
            "https://equipo.red.ts.net:65536",
            "https://equipo.red.ts.net:+80",
            "https://equipo.red.ts.net:",
            "https://127.0.0.1:8787",
            "http://127.0.0.2:8787",
            "http://localhost.example.com",
            "http://8.8.8.8:8787",
            "http://172.15.0.1",
            "http://172.32.0.1",
            "http://192.169.1.1",
            "http://11.0.0.1",
            "http://192.168.1.1@8.8.8.8",
            "http://8.8.8.8@192.168.1.1",
            "http://user@192.168.1.10:8787",
            "http://192.168.1.10/ruta",
            "http://192.168.1.10:8787/x",
            "http://192.168.01.10",
            "http://192.168.1.010",
            "http://010.0.0.1",
            "http://0300.168.1.1",
            "http://0xc0.168.1.1",
            "http://3232235786",
            "http://192.168.266.1",
            "http://192.168.1",
            "http://192.168.1.1.1",
            "http://192.168.1.",
            "http://192.168.1.10:0",
            "http://192.168.1.10:",
            "http://[::ffff:192.168.1.10]:8787",
            "http://[fe80::1]",
            "https://192.168.1.10:8787",
            "http://192.168.1.10.nip.io",
            "http://[::1]:8787",
            "ftp://localhost",
            "https://équipo.red.ts.net",
        ] {
            assert!(
                matches!(BotUrl::parse(typed), Err(BotError::InvalidUrl)),
                "{typed}"
            );
        }
        let long = format!("https://{}.ts.net", "a".repeat(300));
        assert!(BotUrl::parse(&long).is_err());
    }

    fn local_folder(port_file: &str) -> tempfile::TempDir {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join(LOCAL_TOKEN_FILE), format!("{TOKEN}\n")).unwrap();
        fs::write(folder.path().join(LOCAL_PORT_FILE), port_file).unwrap();
        folder
    }

    #[test]
    fn connects_to_the_local_bot_from_its_files() {
        let local = local_folder(r#"{"version":1,"puerto":9001}"#);
        let data = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(data.path().join(CONNECTION_FILE));

        let saved = store.connect_local(local.path()).unwrap().unwrap();

        assert_eq!(saved.url.as_deref(), Some("http://127.0.0.1:9001"));
        assert!(saved.enabled && saved.has_token);
        assert!(!serde_json::to_string(&saved).unwrap().contains(TOKEN));
        assert!(store.client().is_ok());
    }

    #[test]
    fn the_local_bot_never_replaces_a_saved_connection() {
        let local = local_folder(r#"{"version":1,"puerto":9001}"#);
        let data = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(data.path().join(CONNECTION_FILE));
        store
            .connect("https://equipo.red.ts.net", Some(OTHER_TOKEN))
            .unwrap();
        store.disable().unwrap();

        assert_eq!(store.connect_local(local.path()).unwrap(), None);

        assert_eq!(
            store.summary().unwrap().url.as_deref(),
            Some("https://equipo.red.ts.net")
        );
    }

    #[test]
    fn without_local_files_nothing_is_saved() {
        let data = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(data.path().join(CONNECTION_FILE));
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(store.connect_local(empty.path()).unwrap(), None);
        assert_eq!(
            store.connect_local(&empty.path().join("falta")).unwrap(),
            None
        );
        // Only the token, without the port file.
        fs::write(empty.path().join(LOCAL_TOKEN_FILE), TOKEN).unwrap();
        assert_eq!(store.connect_local(empty.path()).unwrap(), None);
        assert_eq!(store.summary().unwrap(), BotConnectionUi::default());
    }

    #[test]
    fn invalid_local_files_are_refused_without_quoting_them() {
        for port_file in [
            "no es json",
            r#"{"version":2,"puerto":9001}"#,
            r#"{"version":1,"puerto":0}"#,
            r#"{"version":1,"puerto":70000}"#,
            r#"{"version":1}"#,
            r#"{"version":1,"puerto":"9001"}"#,
        ] {
            let local = local_folder(port_file);
            let data = tempfile::tempdir().unwrap();
            let store = ConnectionStore::new(data.path().join(CONNECTION_FILE));
            let error = store.connect_local(local.path()).unwrap_err();
            assert!(matches!(error, BotError::InvalidLocalBot), "{port_file}");
            assert_eq!(store.summary().unwrap(), BotConnectionUi::default());
        }
        let local = local_folder(r#"{"version":1,"puerto":9001}"#);
        fs::write(local.path().join(LOCAL_TOKEN_FILE), "corto").unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(data.path().join(CONNECTION_FILE));
        let error = store.connect_local(local.path()).unwrap_err();
        assert!(matches!(error, BotError::InvalidLocalBot));
        assert!(!error.to_string().contains("corto"));
        fs::write(local.path().join(LOCAL_TOKEN_FILE), "x".repeat(5000)).unwrap();
        assert!(store.connect_local(local.path()).is_err());
    }

    #[test]
    fn the_local_folder_follows_xdg_config_home_only_if_absolute() {
        let os = |text: &str| Some(OsString::from(text));
        let expected = |text: &str| Some(PathBuf::from(text));
        assert_eq!(
            bot_folder_in(os("/tmp/cfg"), os("/home/a")),
            expected("/tmp/cfg/bot-stickers-sonido")
        );
        assert_eq!(
            bot_folder_in(os("relativo"), os("/home/a")),
            expected("/home/a/.config/bot-stickers-sonido")
        );
        assert_eq!(
            bot_folder_in(None, os("/home/a")),
            expected("/home/a/.config/bot-stickers-sonido")
        );
        assert_eq!(bot_folder_in(None, os("relativo")), None);
        assert_eq!(bot_folder_in(None, None), None);
    }

    #[test]
    fn the_token_is_43_base64url_characters() {
        assert!(BotToken::parse(&format!("  {TOKEN}\n")).is_ok());
        for typed in [
            String::new(),
            TOKEN[1..].to_string(),
            format!("{TOKEN}a"),
            format!("{}=", &TOKEN[1..]),
            format!("{}\r\nX", &TOKEN[3..]),
            format!("{}ñ", &TOKEN[2..]),
        ] {
            assert!(matches!(
                BotToken::parse(&typed),
                Err(BotError::InvalidToken)
            ));
        }
    }

    #[test]
    fn a_saved_connection_never_gives_the_token_back_and_is_private() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("datos").join(CONNECTION_FILE);
        let store = ConnectionStore::new(path.clone());
        assert_eq!(store.summary().unwrap(), BotConnectionUi::default());

        let saved = store
            .connect("https://equipo.red.ts.net/", Some(TOKEN))
            .unwrap();

        assert_eq!(saved.url.as_deref(), Some("https://equipo.red.ts.net"));
        assert!(saved.has_token && saved.enabled);
        assert!(!serde_json::to_string(&saved).unwrap().contains(TOKEN));
        assert_eq!(store.summary().unwrap(), saved);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let files = fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(files, 1);
    }

    fn link(address: &str, token: &str) -> String {
        format!("conversorstickers://bot#u={address}&t={token}")
    }

    #[test]
    fn a_connection_link_gives_the_address_and_saves_the_connection() {
        let parsed =
            ConnectionLink::parse(&link("https%3A%2F%2Fequipo.red.ts.net", TOKEN)).unwrap();
        assert_eq!(parsed.url(), "https://equipo.red.ts.net");
        let local = ConnectionLink::parse(&link("http%3A%2F%2F127.0.0.1%3A8787", TOKEN)).unwrap();
        assert_eq!(local.url(), "http://127.0.0.1:8787");
        // The order of the parameters does not matter and the ends are trimmed.
        let reversed =
            format!(" conversorstickers://bot#t={TOKEN}&u=http%3A%2F%2Flocalhost%3A8787\n");
        assert_eq!(
            ConnectionLink::parse(&reversed).unwrap().url(),
            "http://localhost:8787"
        );

        let folder = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(folder.path().join(CONNECTION_FILE));
        let saved = store.connect_link(parsed).unwrap();
        assert_eq!(saved.url.as_deref(), Some("https://equipo.red.ts.net"));
        assert!(saved.has_token && saved.enabled);
        assert!(!serde_json::to_string(&saved).unwrap().contains(TOKEN));
    }

    #[test]
    fn a_received_link_never_replaces_an_enabled_connection() {
        let folder = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(folder.path().join(CONNECTION_FILE));
        let first = ConnectionLink::parse(&link("https%3A%2F%2Funo.red.ts.net", TOKEN)).unwrap();
        let second =
            ConnectionLink::parse(&link("https%3A%2F%2Fdos.red.ts.net", OTHER_TOKEN)).unwrap();
        store.connect_received_link(first).unwrap();
        assert!(matches!(
            store.ensure_not_enabled(),
            Err(BotError::AlreadyConnected)
        ));
        assert!(matches!(
            store.connect_received_link(second),
            Err(BotError::AlreadyConnected)
        ));
        let kept = store.summary().unwrap();
        assert_eq!(kept.url.as_deref(), Some("https://uno.red.ts.net"));
        // Once disabled, another link is accepted.
        store.disable().unwrap();
        store.ensure_not_enabled().unwrap();
        let third =
            ConnectionLink::parse(&link("https%3A%2F%2Fdos.red.ts.net", OTHER_TOKEN)).unwrap();
        let saved = store.connect_received_link(third).unwrap();
        assert_eq!(saved.url.as_deref(), Some("https://dos.red.ts.net"));
    }

    #[test]
    fn a_connection_link_follows_the_same_rules_as_typing() {
        let good = "https%3A%2F%2Fequipo.red.ts.net";
        let url = || "InvalidUrl";
        let token = || "InvalidToken";
        let shape = || "InvalidConnectionLink";
        let cases = [
            (link("http%3A%2F%2Fexample.com", TOKEN), url()),
            (link("https%3A%2F%2Fexample.com", TOKEN), url()),
            (link("https%3A%2F%2Fequipo.red.ts.net%2Fruta", TOKEN), url()),
            (link("https%3A%2F%2Fuser%40equipo.red.ts.net", TOKEN), url()),
            (link(good, "corto"), token()),
            (link(good, &format!("{TOKEN}x")), token()),
            (link("https%3A%2F%2Fequipo.red.ts.net%zz", TOKEN), shape()),
            (format!("conversorstickers://bot#u={good}"), shape()),
            (
                format!("conversorstickers://bot#u={good}&u={good}&t={TOKEN}"),
                shape(),
            ),
            (
                format!("conversorstickers://bot#u={good}&t={TOKEN}&x=1"),
                shape(),
            ),
            (format!("https://bot#u={good}&t={TOKEN}"), shape()),
        ];
        for (text, expected) in &cases {
            let error = ConnectionLink::parse(text).err().unwrap();
            assert_eq!(format!("{error:?}"), *expected);
            assert!(!error.to_string().contains(TOKEN));
        }
        let long = link(&"a".repeat(MAX_CONNECTION_LINK_LENGTH), TOKEN);
        assert!(ConnectionLink::parse(&long).is_err());
        assert!(ConnectionLink::parse("").is_err());
    }

    #[test]
    fn a_pending_link_is_confirmed_once_and_the_last_one_wins() {
        let pending = PendingLink::default();
        assert!(pending.take().is_none());
        let first = ConnectionLink::parse(&link("https%3A%2F%2Funo.red.ts.net", TOKEN)).unwrap();
        let second =
            ConnectionLink::parse(&link("https%3A%2F%2Fdos.red.ts.net", OTHER_TOKEN)).unwrap();
        assert_eq!(pending.offer(first), "https://uno.red.ts.net");
        assert_eq!(pending.offer(second), "https://dos.red.ts.net");
        assert_eq!(pending.take().unwrap().url(), "https://dos.red.ts.net");
        assert!(pending.take().is_none());
    }

    #[test]
    fn another_address_needs_the_token_again() {
        let folder = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(folder.path().join(CONNECTION_FILE));
        assert!(matches!(
            store.connect("http://127.0.0.1:8787", None),
            Err(BotError::TokenRequired)
        ));
        store.connect("http://127.0.0.1:8787", Some(TOKEN)).unwrap();
        store.disable().unwrap();

        for other in ["http://localhost:8787", "https://equipo.red.ts.net"] {
            assert!(matches!(
                store.connect(other, None),
                Err(BotError::TokenRequired)
            ));
        }
        // A failed attempt changes nothing.
        let kept = store.summary().unwrap();
        assert_eq!(kept.url.as_deref(), Some("http://127.0.0.1:8787"));
        assert!(!kept.enabled);
        assert!(
            store
                .connect("http://127.0.0.1:8787/", Some("  "))
                .unwrap()
                .enabled
        );
        assert!(
            store
                .connect("https://equipo.red.ts.net", Some(OTHER_TOKEN))
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn disabling_keeps_the_connection_and_stops_the_client() {
        let folder = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(folder.path().join(CONNECTION_FILE));
        assert_eq!(store.disable().unwrap(), BotConnectionUi::default());
        assert!(matches!(store.client(), Err(BotError::NotEnabled)));
        store.connect("http://127.0.0.1:8787", Some(TOKEN)).unwrap();
        assert!(store.client().is_ok());

        let disabled = store.disable().unwrap();

        assert!(disabled.has_token && !disabled.enabled);
        assert!(matches!(store.client(), Err(BotError::NotEnabled)));
    }

    #[test]
    fn a_damaged_or_tampered_file_is_an_error_that_a_new_token_fixes() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(CONNECTION_FILE);
        let store = ConnectionStore::new(path.clone());
        for text in [
            "no es json".to_string(),
            format!(r#"{{"url":"https://example.com","token":"{TOKEN}","enabled":true}}"#),
        ] {
            fs::write(&path, text).unwrap();
            let error = store.summary().unwrap_err();
            assert!(matches!(error, BotError::CorruptConnection));
            assert!(!error.to_string().contains(TOKEN));
            assert!(store.client().is_err());
        }
        assert!(store.connect("http://localhost:8787", Some(TOKEN)).is_ok());
    }

    #[test]
    fn asks_for_the_status_with_the_token() {
        let (url, served) = fake_bot(http("200 OK", PAUSED));

        let status = client(&url).status().unwrap();

        let request = served.join().unwrap().to_ascii_lowercase();
        assert!(request.starts_with("get /estado http/1.1\r\n"));
        assert!(request.contains(&format!(
            "authorization: bearer {}\r\n",
            TOKEN.to_ascii_lowercase()
        )));
        assert!(!request.contains("origin:"));
        assert!(status.link == LinkState::Linked);
        assert!(status.connection == BotLink::Connected);
        assert!(!status.active);
    }

    #[test]
    fn each_operation_uses_its_method_and_path() {
        type Operation = fn(&BotClient) -> Result<BotStatusUi, BotError>;
        let operations: [(Operation, &str, &str); 5] = [
            (BotClient::activate, "POST /activar ", ""),
            (BotClient::pause, "POST /pausar ", ""),
            (
                |client| client.link(LinkMethod::Code, Some("34600111222")),
                "POST /vinculacion ",
                r#"{"metodo":"codigo","numero":"34600111222"}"#,
            ),
            (
                |client| client.link(LinkMethod::Qr, None),
                "POST /vinculacion ",
                r#"{"metodo":"qr"}"#,
            ),
            (BotClient::unlink, "DELETE /vinculacion ", ""),
        ];
        for (operation, start, body) in operations {
            let (url, served) = fake_bot(http("202 Accepted", PAUSED));
            assert!(operation(&client(&url)).is_ok(), "{start}");
            let request = served.join().unwrap();
            assert!(request.starts_with(start), "{request}");
            assert!(request.ends_with(&format!("\r\n\r\n{body}")), "{request}");
        }
    }

    #[test]
    fn a_wrong_token_and_too_many_attempts_are_told_apart() {
        assert!(matches!(
            failure(answer("401 Unauthorized", r#"{"error":"no"}"#)),
            BotError::Unauthorized
        ));
        assert!(matches!(
            failure(answer("429 Too Many Requests", "")),
            BotError::TooManyAttempts
        ));
    }

    #[test]
    fn the_bot_explains_its_errors_cut_short_and_on_one_line() {
        let body = format!(
            r#"{{"error":"Sin conexión\ncon WhatsApp {}"}}"#,
            "x".repeat(500)
        );
        let BotError::Refused(message) = failure(answer("409 Conflict", &body)) else {
            panic!("a refusal was expected");
        };
        assert!(message.starts_with("Sin conexión con WhatsApp x"));
        assert_eq!(message.chars().count(), MAX_BOT_TEXT_CHARS);
        assert!(matches!(
            failure(answer("500 Internal Server Error", "<html>")),
            BotError::UnexpectedStatus(500)
        ));
    }

    #[test]
    fn a_redirect_is_not_followed() {
        let other = TcpListener::bind("127.0.0.1:0").unwrap();
        other.set_nonblocking(true).unwrap();
        let (url, served) = fake_bot(format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/estado\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            other.local_addr().unwrap().port()
        ));

        let error = failure(client(&url).status());

        served.join().unwrap();
        assert!(matches!(error, BotError::UnexpectedStatus(302)), "{error}");
        assert_eq!(
            other.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn a_huge_response_is_cut_off_with_or_without_a_declared_length() {
        let huge = "x".repeat(MAX_RESPONSE_BYTES as usize + 1);
        assert!(matches!(
            failure(answer("200 OK", &huge)),
            BotError::TooLarge
        ));
        let (url, served) = fake_bot(format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{huge}"
        ));
        assert!(matches!(failure(client(&url).status()), BotError::TooLarge));
        served.join().unwrap();
    }

    #[test]
    fn invalid_json_and_other_versions_are_rejected() {
        for body in ["no es json", "{}", r#"{"version":1}"#] {
            assert!(matches!(
                failure(answer("200 OK", body)),
                BotError::InvalidResponse
            ));
        }
        assert!(matches!(
            failure(answer("200 OK", r#"{"version":2,"estado":"otro"}"#)),
            BotError::IncompatibleVersion(2)
        ));
        assert!(matches!(
            failure(answer(
                "200 OK",
                &PAUSED.replace("\"vinculado\"", "\"otra-cosa\"")
            )),
            BotError::InvalidResponse
        ));
    }

    #[test]
    fn code_and_qr_only_while_linking_and_with_their_shape() {
        let status = |link: &str, code: &str, qr: &str| {
            parse_status(
                format!(
                    r#"{{"version":1,"vinculacion":"{link}","conexion":"conectando","activo":false,"codigo":{code},"qr":{qr},"aviso":"  "}}"#
                )
                .as_bytes(),
            )
        };
        let linking = status("vinculando", r#""AB12-EF34""#, r#""2@abc,def=""#).unwrap();
        assert_eq!(linking.code.as_deref(), Some("AB12-EF34"));
        assert_eq!(linking.qr.as_deref(), Some("2@abc,def="));
        assert_eq!(linking.notice, None);
        let linked = status("vinculado", r#""AB12-EF34""#, r#""2@abc""#).unwrap();
        assert!(linked.code.is_none() && linked.qr.is_none());
        for code in [r#""<img src=x>""#, r#""ABCDEFGHI""#, r#""abcd-efgh""#] {
            assert!(status("vinculando", code, "null").is_err(), "{code}");
        }
        let long = format!("\"{}\"", "a".repeat(MAX_QR_LENGTH + 1));
        assert!(status("vinculando", "null", &long).is_err());
    }

    #[test]
    fn nobody_listening_is_unreachable() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let error = failure(client(&format!("http://127.0.0.1:{port}")).status());
        assert!(matches!(error, BotError::Unreachable));
        assert_eq!(UiError::from(error).kind, UiErrorKind::BotUnreachable);
        assert_eq!(
            UiError::from(BotError::Unauthorized).kind,
            UiErrorKind::BotUnauthorized
        );
    }
}
