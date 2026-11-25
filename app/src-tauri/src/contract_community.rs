//! Community types that cross the boundary with the interface. Each one has a copy in
//! `app/src/contract-community.ts`, checked against `app/src/contract-community.examples.json`.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CommunityCategoryUi {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CommunityPackUi {
    pub(crate) id: String,
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) license: String,
    pub(crate) category: String,
    pub(crate) tags: Vec<String>,
    pub(crate) animated: bool,
    pub(crate) added: String,
    pub(crate) stickers: u32,
    pub(crate) icon: Option<String>,
    pub(crate) downloaded: Option<u32>,
    pub(crate) updatable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CatalogUi {
    pub(crate) categories: Vec<CommunityCategoryUi>,
    pub(crate) packs: Vec<CommunityPackUi>,
    pub(crate) updated: u64,
    pub(crate) offline: bool,
    pub(crate) notice_accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SampleUi {
    pub(crate) path: String,
    pub(crate) emojis: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CommunityDetailUi {
    pub(crate) pack: CommunityPackUi,
    pub(crate) stickers: Vec<SampleUi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum CommunityProgressUi {
    Downloading { done: u32, total: u32 },
    Verifying,
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    const EXAMPLES: &str = include_str!("../../src/contract-community.examples.json");

    fn example(name: &str) -> Value {
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        examples
            .get(name)
            .unwrap_or_else(|| panic!("falta el ejemplo {name}"))
            .clone()
    }

    fn pack() -> CommunityPackUi {
        CommunityPackUi {
            id: "gatos-garabato".to_string(),
            version: 2,
            name: "Gatos garabato".to_string(),
            author: "Ana".to_string(),
            license: "CC-BY-4.0".to_string(),
            category: "animales".to_string(),
            tags: vec!["gato".to_string(), "garabato".to_string()],
            animated: false,
            added: "2026-11-22".to_string(),
            stickers: 3,
            icon: Some("/cache/comunidad/archivos/ab12.png".to_string()),
            downloaded: Some(7),
            updatable: true,
        }
    }

    #[test]
    fn community_pack_ui() {
        assert_eq!(
            serde_json::to_value(pack()).unwrap(),
            example("community_pack_ui")
        );
        let without_icon = CommunityPackUi {
            icon: None,
            downloaded: None,
            updatable: false,
            ..pack()
        };
        assert_eq!(
            serde_json::to_value(without_icon).unwrap(),
            example("community_pack_not_downloaded")
        );
    }

    #[test]
    fn catalog_ui() {
        let catalog = CatalogUi {
            categories: vec![CommunityCategoryUi {
                id: "animales".to_string(),
                name: "Animales".to_string(),
            }],
            packs: vec![pack()],
            updated: 1_763_800_000,
            offline: false,
            notice_accepted: true,
        };
        assert_eq!(
            serde_json::to_value(catalog).unwrap(),
            example("catalog_ui")
        );
    }

    #[test]
    fn community_detail_ui() {
        let detail = CommunityDetailUi {
            pack: pack(),
            stickers: vec![SampleUi {
                path: "/cache/comunidad/archivos/cd34.webp".to_string(),
                emojis: vec!["🐱".to_string()],
            }],
        };
        assert_eq!(
            serde_json::to_value(detail).unwrap(),
            example("community_detail_ui")
        );
    }

    #[test]
    fn download_progress() {
        assert_eq!(
            serde_json::to_value(CommunityProgressUi::Downloading { done: 2, total: 4 }).unwrap(),
            example("progress_downloading")
        );
        assert_eq!(
            serde_json::to_value(CommunityProgressUi::Verifying).unwrap(),
            example("progress_verifying")
        );
    }
}
