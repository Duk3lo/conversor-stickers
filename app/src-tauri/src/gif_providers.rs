//! Chosen GIF provider and the user's keys: `providers.json` in `app_config_dir`, with 0600
//! permissions on Unix. The key is never logged nor shown in an error. KLIPY also has a key baked
//! in at build time (`build.rs`) and, failing both, the app's proxy (`CONVERSOR_PROXY_URL`) can
//! serve KLIPY without any key in the app. Precedence: the user's key, the embedded one, the proxy.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use tracing::warn;

use crate::gifs::{GifError, validate_key, validate_proxy};

const FILE_NAME: &str = "providers.json";

const BUNDLED_KLIPY_KEY: Option<&str> = option_env!("BUNDLED_KLIPY_KEY");
const PROXY_URL: Option<&str> = option_env!("CONVERSOR_PROXY_URL");

/// How a provider is reached: with a key of its own or through the app's proxy.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Credential {
    Key(String),
    Proxy(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Provider {
    Klipy,
    Giphy,
}

impl Provider {
    fn uppercase(self) -> &'static str {
        match self {
            Provider::Klipy => "KLIPY",
            Provider::Giphy => "GIPHY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KeyOrigin {
    Bundled,
    User,
    Proxy,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    provider: Option<Provider>,
    klipy: Option<String>,
    giphy: Option<String>,
}

impl File {
    fn key(&self, provider: Provider) -> Option<&str> {
        match provider {
            Provider::Klipy => self.klipy.as_deref(),
            Provider::Giphy => self.giphy.as_deref(),
        }
    }

    fn put_key(&mut self, provider: Provider, key: Option<String>) {
        match provider {
            Provider::Klipy => self.klipy = key,
            Provider::Giphy => self.giphy = key,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProviderState {
    pub(crate) provider: Option<Provider>,
    pub(crate) key_set: bool,
    pub(crate) key_origin: Option<KeyOrigin>,
}

pub(crate) struct Providers {
    path: PathBuf,
    bundled: Option<&'static str>,
    proxy: Option<String>,
}

fn disk(action: &'static str) -> impl FnOnce(io::Error) -> GifError {
    move |source| GifError::Disk { action, source }
}

impl Providers {
    pub(crate) fn new(config: PathBuf) -> Self {
        let proxy = PROXY_URL.and_then(|url| match validate_proxy(url) {
            Ok(url) => Some(url),
            Err(error) => {
                warn!(%error, "CONVERSOR_PROXY_URL ignorada");
                None
            }
        });
        Self::with_bundled(config, BUNDLED_KLIPY_KEY, proxy)
    }

    fn with_bundled(config: PathBuf, bundled: Option<&'static str>, proxy: Option<String>) -> Self {
        Providers {
            path: config.join(FILE_NAME),
            bundled,
            proxy,
        }
    }

    fn key_in_use(&self, file: &File, provider: Provider) -> Option<(Credential, KeyOrigin)> {
        if let Some(key) = file.key(provider) {
            return Some((Credential::Key(key.to_owned()), KeyOrigin::User));
        }
        if provider != Provider::Klipy {
            return None;
        }
        match (self.bundled, &self.proxy) {
            (Some(key), _) => Some((Credential::Key(key.to_owned()), KeyOrigin::Bundled)),
            (None, Some(proxy)) => Some((Credential::Proxy(proxy.clone()), KeyOrigin::Proxy)),
            (None, None) => None,
        }
    }

    fn read(&self) -> Result<File, GifError> {
        let text = match fs::read(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(File::default()),
            Err(source) => return Err(disk("leer los ajustes de GIF")(source)),
        };
        serde_json::from_slice(&text).map_err(|error| {
            disk("leer los ajustes de GIF")(io::Error::new(io::ErrorKind::InvalidData, error))
        })
    }

    fn save(&self, file: &File) -> Result<(), GifError> {
        let action = "guardar los ajustes de GIF";
        let bytes = serde_json::to_vec_pretty(file)
            .map_err(|error| disk(action)(io::Error::other(error)))?;
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory).map_err(disk(action))?;
        }
        let temp = self.path.with_extension("json.tmp");
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let result = (|| {
            let mut destination = options.open(&temp)?;
            destination.write_all(&bytes)?;
            destination.sync_all()?;
            fs::rename(&temp, &self.path)
        })();
        if let Err(source) = result {
            let _ = fs::remove_file(&temp);
            return Err(disk(action)(source));
        }
        Ok(())
    }

    pub(crate) fn state(&self) -> Result<ProviderState, GifError> {
        let file = self.read()?;
        let key_origin = self
            .key_in_use(&file, file.provider.unwrap_or(Provider::Klipy))
            .map(|(_, origin)| origin);
        Ok(ProviderState {
            provider: file.provider,
            key_set: key_origin.is_some(),
            key_origin,
        })
    }

    pub(crate) fn configure(&self, provider: Provider, key: Option<&str>) -> Result<(), GifError> {
        let key = key
            .map(|key| {
                let key = key.trim();
                validate_key(key).map(|()| key.to_owned())
            })
            .transpose()?;
        let mut file = self.read()?;
        file.provider = Some(provider);
        file.put_key(provider, key);
        self.save(&file)
    }

    pub(crate) fn klipy_access(&self) -> Result<Credential, GifError> {
        let file = self.read()?;
        self.key_in_use(&file, Provider::Klipy)
            .map(|(key, _)| key)
            .ok_or(GifError::MissingKey {
                provider: Provider::Klipy.uppercase(),
            })
    }

    pub(crate) fn active(&self) -> Result<(Provider, Credential), GifError> {
        let file = self.read()?;
        let provider = file.provider.unwrap_or(Provider::Klipy);
        match self.key_in_use(&file, provider) {
            Some((key, _)) => Ok((provider, key)),
            None => Err(GifError::MissingKey {
                provider: provider.uppercase(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new() -> (tempfile::TempDir, Providers) {
        let directory = tempfile::tempdir().unwrap();
        let providers = Providers::with_bundled(directory.path().join("config"), None, None);
        (directory, providers)
    }

    #[test]
    fn without_file_there_is_no_provider_and_it_asks_for_klipy_key() {
        let (_directory, providers) = new();
        assert_eq!(
            providers.state().unwrap(),
            ProviderState {
                provider: None,
                key_set: false,
                key_origin: None
            }
        );
        assert_eq!(
            providers.active().unwrap_err().to_string(),
            "Falta la clave de KLIPY"
        );
    }

    #[test]
    fn configure_saves_provider_and_key_and_can_clear() {
        let (_directory, providers) = new();
        providers
            .configure(Provider::Giphy, Some("  abc-123  "))
            .unwrap();
        assert_eq!(
            providers.state().unwrap(),
            ProviderState {
                provider: Some(Provider::Giphy),
                key_set: true,
                key_origin: Some(KeyOrigin::User)
            }
        );
        assert_eq!(
            providers.active().unwrap(),
            (Provider::Giphy, Credential::Key("abc-123".to_owned()))
        );

        providers.configure(Provider::Klipy, Some("otra")).unwrap();
        providers.configure(Provider::Giphy, None).unwrap();
        assert_eq!(
            providers.active().unwrap_err().to_string(),
            "Falta la clave de GIPHY"
        );
        providers.configure(Provider::Klipy, None).unwrap();
        assert!(!providers.state().unwrap().key_set);
    }

    #[test]
    fn key_with_odd_characters_is_rejected_unsaved() {
        let (_directory, providers) = new();
        let error = providers
            .configure(Provider::Klipy, Some("a/b"))
            .unwrap_err();
        assert_eq!(error.to_string(), "La clave no es válida");
        assert_eq!(providers.state().unwrap().provider, None);
    }

    #[test]
    fn corrupt_file_is_an_error_and_not_overwritten_silently() {
        let (_directory, providers) = new();
        fs::create_dir_all(providers.path.parent().unwrap()).unwrap();
        fs::write(&providers.path, b"no es json").unwrap();
        assert!(providers.state().is_err());
        assert!(providers.configure(Provider::Klipy, Some("abc")).is_err());
        assert_eq!(fs::read(&providers.path).unwrap(), b"no es json");
    }

    #[cfg(unix)]
    #[test]
    fn file_has_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (_directory, providers) = new();
        providers.configure(Provider::Klipy, Some("abc")).unwrap();
        let mode = fs::metadata(&providers.path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        providers.configure(Provider::Klipy, Some("def")).unwrap();
        let mode = fs::metadata(&providers.path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn bundled_key_serves_klipy_and_user_key_wins() {
        let directory = tempfile::tempdir().unwrap();
        let providers =
            Providers::with_bundled(directory.path().join("config"), Some("integrada"), None);
        assert_eq!(
            providers.active().unwrap(),
            (Provider::Klipy, Credential::Key("integrada".to_owned()))
        );
        let state = providers.state().unwrap();
        assert!(state.key_set);
        assert_eq!(state.key_origin, Some(KeyOrigin::Bundled));

        providers.configure(Provider::Klipy, Some("mia")).unwrap();
        assert_eq!(
            providers.active().unwrap(),
            (Provider::Klipy, Credential::Key("mia".to_owned()))
        );
        assert_eq!(providers.state().unwrap().key_origin, Some(KeyOrigin::User));

        providers.configure(Provider::Klipy, None).unwrap();
        assert_eq!(
            providers.state().unwrap().key_origin,
            Some(KeyOrigin::Bundled)
        );
    }

    #[test]
    fn stickers_use_klipy_key_even_if_provider_is_giphy() {
        let (_directory, providers) = new();
        assert_eq!(
            providers.klipy_access().unwrap_err().to_string(),
            "Falta la clave de KLIPY"
        );
        providers.configure(Provider::Klipy, Some("mia")).unwrap();
        providers.configure(Provider::Giphy, Some("otra")).unwrap();
        assert_eq!(
            providers.klipy_access().unwrap(),
            Credential::Key("mia".to_owned())
        );
    }

    #[test]
    fn bundled_key_is_not_used_with_giphy() {
        let directory = tempfile::tempdir().unwrap();
        let providers =
            Providers::with_bundled(directory.path().join("config"), Some("integrada"), None);
        providers.configure(Provider::Giphy, None).unwrap();
        assert_eq!(
            providers.active().unwrap_err().to_string(),
            "Falta la clave de GIPHY"
        );
        assert_eq!(providers.state().unwrap().key_origin, None);
    }

    fn with_proxy(key: Option<&'static str>) -> (tempfile::TempDir, Providers) {
        let directory = tempfile::tempdir().unwrap();
        let proxy = Some("https://proxy.example".to_owned());
        let providers = Providers::with_bundled(directory.path().join("config"), key, proxy);
        (directory, providers)
    }

    #[test]
    fn proxy_serves_klipy_when_there_is_no_key_and_is_a_valid_state() {
        let (_directory, providers) = with_proxy(None);
        let proxy = Credential::Proxy("https://proxy.example".to_owned());
        assert_eq!(providers.active().unwrap(), (Provider::Klipy, proxy));
        assert_eq!(
            providers.klipy_access().unwrap(),
            Credential::Proxy("https://proxy.example".to_owned())
        );
        let state = providers.state().unwrap();
        assert!(state.key_set);
        assert_eq!(state.key_origin, Some(KeyOrigin::Proxy));
    }

    #[test]
    fn user_and_bundled_keys_win_over_the_proxy() {
        let (_directory, providers) = with_proxy(Some("integrada"));
        assert_eq!(
            providers.klipy_access().unwrap(),
            Credential::Key("integrada".to_owned())
        );
        providers.configure(Provider::Klipy, Some("mia")).unwrap();
        assert_eq!(
            providers.klipy_access().unwrap(),
            Credential::Key("mia".to_owned())
        );
        assert_eq!(providers.state().unwrap().key_origin, Some(KeyOrigin::User));
    }

    #[test]
    fn proxy_is_not_used_with_giphy() {
        let (_directory, providers) = with_proxy(None);
        providers.configure(Provider::Giphy, None).unwrap();
        assert_eq!(
            providers.active().unwrap_err().to_string(),
            "Falta la clave de GIPHY"
        );
        // Stickers still go through the proxy.
        assert!(matches!(
            providers.klipy_access().unwrap(),
            Credential::Proxy(_)
        ));
    }
}
