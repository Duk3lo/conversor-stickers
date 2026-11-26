//! What the «Mi bot» panel gets and sends. The TypeScript side is `contract-bot.ts` and the shared
//! examples are `contract-bot.examples.json`. The token never travels to the interface.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct BotConnectionUi {
    pub(crate) url: Option<String>,
    pub(crate) has_token: bool,
    pub(crate) enabled: bool,
}

/// Deserialized from the words of the bot's API and serialized with the app's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LinkState {
    #[serde(rename(serialize = "unlinked", deserialize = "sin-vincular"))]
    Unlinked,
    #[serde(rename(serialize = "linking", deserialize = "vinculando"))]
    Linking,
    #[serde(rename(serialize = "linked", deserialize = "vinculado"))]
    Linked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum BotLink {
    #[serde(rename(serialize = "disconnected", deserialize = "desconectado"))]
    Disconnected,
    #[serde(rename(serialize = "connecting", deserialize = "conectando"))]
    Connecting,
    #[serde(rename(serialize = "connected", deserialize = "conectado"))]
    Connected,
}

/// No `Debug`: `code` and `qr` link a WhatsApp account and must not reach any log.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BotStatusUi {
    pub(crate) link: LinkState,
    pub(crate) connection: BotLink,
    pub(crate) active: bool,
    /// Pairing code, only while linking.
    pub(crate) code: Option<String>,
    /// Text to draw as a QR, only while linking.
    pub(crate) qr: Option<String>,
    /// Why the bot is as it is, in the bot's words: untrusted text, only to be shown as text.
    pub(crate) notice: Option<String>,
}

/// How the person wants to link the bot's WhatsApp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LinkMethod {
    #[serde(rename(serialize = "codigo", deserialize = "code"))]
    Code,
    #[serde(rename = "qr")]
    Qr,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn example(name: &str) -> Value {
        let examples: Value =
            serde_json::from_str(include_str!("../../src/contract-bot.examples.json")).unwrap();
        examples[name].clone()
    }

    #[test]
    fn connection_and_statuses_match_the_shared_examples() {
        let connection = BotConnectionUi {
            url: Some("https://equipo.red-ejemplo.ts.net".to_string()),
            has_token: true,
            enabled: true,
        };
        assert_eq!(
            serde_json::to_value(connection).unwrap(),
            example("connection")
        );
        assert_eq!(
            serde_json::to_value(BotConnectionUi::default()).unwrap(),
            example("without_connection")
        );
        let linking = BotStatusUi {
            link: LinkState::Linking,
            connection: BotLink::Connecting,
            active: false,
            code: Some("ABCD-EFGH".to_string()),
            qr: None,
            notice: None,
        };
        assert_eq!(serde_json::to_value(linking).unwrap(), example("linking"));
        let active = BotStatusUi {
            link: LinkState::Linked,
            connection: BotLink::Connected,
            active: true,
            code: None,
            qr: None,
            notice: Some("Aviso del bot".to_string()),
        };
        assert_eq!(serde_json::to_value(active).unwrap(), example("active"));
    }

    #[test]
    fn link_methods_come_from_the_interface_and_go_to_the_bot_in_its_words() {
        let methods: Vec<LinkMethod> = serde_json::from_value(example("link_methods")).unwrap();
        assert_eq!(methods, [LinkMethod::Code, LinkMethod::Qr]);
        assert_eq!(
            serde_json::to_value(methods).unwrap(),
            json!(["codigo", "qr"])
        );
    }
}
