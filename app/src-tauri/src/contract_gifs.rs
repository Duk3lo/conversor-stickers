//! GIF search types that cross the boundary with the interface. Each one has a copy in
//! `app/src/contract-gifs.ts`, checked against `app/src/contract-gifs.examples.json`.

use serde::Serialize;

use crate::gif_providers::{KeyOrigin, Provider};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct GifProviderUi {
    pub(crate) provider: Option<Provider>,
    pub(crate) key_set: bool,
    pub(crate) key_origin: Option<KeyOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct GifResultUi {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) thumbnail: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct GifsPageUi {
    pub(crate) results: Vec<GifResultUi>,
    pub(crate) next: Option<u32>,
    pub(crate) attribution: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct KlipyStickerUi {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) thumbnail: String,
    pub(crate) animated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct StickersPageUi {
    pub(crate) results: Vec<KlipyStickerUi>,
    pub(crate) next: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct KlipyCategoryUi {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    const EXAMPLES: &str = include_str!("../../src/contract-gifs.examples.json");

    fn example(name: &str) -> Value {
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        examples
            .get(name)
            .unwrap_or_else(|| panic!("falta el ejemplo {name}"))
            .clone()
    }

    #[test]
    fn gif_provider_ui() {
        let with_key = GifProviderUi {
            provider: Some(Provider::Klipy),
            key_set: true,
            key_origin: Some(KeyOrigin::Bundled),
        };
        assert_eq!(
            serde_json::to_value(with_key).unwrap(),
            example("gif_provider_ui")
        );
        let nothing = GifProviderUi {
            provider: None,
            key_set: false,
            key_origin: None,
        };
        assert_eq!(
            serde_json::to_value(nothing).unwrap(),
            example("gif_provider_unset")
        );
        let proxy = GifProviderUi {
            provider: Some(Provider::Klipy),
            key_set: true,
            key_origin: Some(KeyOrigin::Proxy),
        };
        assert_eq!(
            serde_json::to_value(proxy).unwrap(),
            example("gif_provider_proxy")
        );
        assert_eq!(
            serde_json::to_value(Provider::Giphy).unwrap(),
            example("gif_provider_giphy")
        );
    }

    #[test]
    fn gifs_page_ui() {
        let page = GifsPageUi {
            results: vec![GifResultUi {
                id: "8041071659142944".to_string(),
                title: "Hello".to_string(),
                thumbnail: "/cache/gifs/miniaturas/klipy-8041071659142944.webp".to_string(),
                width: 220,
                height: 220,
            }],
            next: Some(2),
            attribution: "Powered by KLIPY".to_string(),
        };
        assert_eq!(serde_json::to_value(page).unwrap(), example("gifs_page_ui"));
        let last = GifsPageUi {
            results: Vec::new(),
            next: None,
            attribution: "Powered by GIPHY".to_string(),
        };
        assert_eq!(
            serde_json::to_value(last).unwrap(),
            example("gifs_page_last")
        );
    }

    #[test]
    fn klipy_stickers_ui() {
        let page = StickersPageUi {
            results: vec![KlipyStickerUi {
                id: "6914457558367152".to_string(),
                title: "Grrr Rawr Sticker".to_string(),
                thumbnail: "/cache/gifs/stickers/miniaturas/klipy-6914457558367152.webp"
                    .to_string(),
                animated: true,
            }],
            next: Some(2),
        };
        assert_eq!(
            serde_json::to_value(page).unwrap(),
            example("stickers_page_ui")
        );
        let categories = vec![KlipyCategoryUi {
            id: "smile".to_string(),
            name: "Smile".to_string(),
        }];
        assert_eq!(
            serde_json::to_value(categories).unwrap(),
            example("klipy_categories_ui")
        );
    }
}
