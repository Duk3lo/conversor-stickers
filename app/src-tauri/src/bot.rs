//! Client of the control API (version 1) of the user's own sticker bot, and the saved connection to
//! it. The bot is reached through Tailscale or on this machine; everything it answers is untrusted.
//! Nothing here logs the token, the pairing code, the QR or phone numbers.

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
const CONNECTION_FILE: &str = "bot-connection.json";

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

    #[error("escribe el token: al cambiar la dirección hay que volver a pegarlo")]
    TokenRequired,

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
            BotError::InvalidUrl | BotError::InvalidToken | BotError::TokenRequired => {
                UiErrorKind::InvalidEntry
            }
            BotError::Unreachable => UiErrorKind::BotUnreachable,
            BotError::Unauthorized => UiErrorKind::BotUnauthorized,
            _ => UiErrorKind::Other,
        };
        UiError::new(kind, error.to_string())
    }
}

/// The base address of the bot, normalized: `https://<host>.ts.net[:port]`, or `http` to this
/// machine only. No user info, path, query or fragment.
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
            "http" => host == "127.0.0.1" || host == "localhost",
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
struct LinkRequest {
    metodo: LinkMethod,
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

    pub(crate) fn link(&self, method: LinkMethod) -> Result<BotStatusUi, BotError> {
        let body = serde_json::to_string(&LinkRequest { metodo: method })
            .map_err(|_| BotError::InvalidResponse)?;
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
            "http://192.168.1.10:8787",
            "http://localhost.example.com",
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
                |client| client.link(LinkMethod::Code),
                "POST /vinculacion ",
                r#"{"metodo":"codigo"}"#,
            ),
            (
                |client| client.link(LinkMethod::Qr),
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
