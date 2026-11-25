//! Types that cross the boundary with the interface. Each one has a copy in `app/src/contract.ts`,
//! and the tests check both sides against the same example JSON (`app/src/contract.examples.json`).
//!
//! Enums with data carry a `kind` tag; enums with only unit variants serialize as a string.
//! Anything that comes from the interface rejects unknown fields.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct PresetUi {
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) extension: String,
    pub(crate) side: u32,
    pub(crate) max_size_bytes: u64,
    pub(crate) max_duration_s: f64,
    pub(crate) max_fps: u32,
    pub(crate) fps_range: FpsRangeUi,
    /// Free output sides the destination accepts; `None` when its canvas is fixed.
    pub(crate) side_range_px: Option<SideRangeUi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct FpsRangeUi {
    pub(crate) min: u32,
    pub(crate) max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SideRangeUi {
    pub(crate) min: u32,
    pub(crate) max: u32,
    pub(crate) suggested: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct EntryUi {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) origin: Origin,
    pub(crate) state: EntryState,
    pub(crate) media: Option<MediaUi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Origin {
    File,
    Link,
    Pasted,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum EntryState {
    Pending,
    Downloading { bytes: u64, total: Option<u64> },
    Ready,
    Error { error: UiError },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum MediaUi {
    Video {
        duration_s: f64,
        width: u32,
        height: u32,
        has_audio: bool,
    },
    Image {
        path: String,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum QueueEvent {
    Change { entry: EntryUi },
    Finished,
}

/// Progress of «save video»: first the download with audio, then writing it where it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum SaveVideoEvent {
    Downloading { bytes: u64, total: Option<u64> },
    Writing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PriorityUi {
    Sharpness,
    Balance,
    Smoothness,
    FitAnyway,
    Fluid,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) struct CreateRequest {
    pub(crate) id: u32,
    pub(crate) preset: String,
    pub(crate) start_s: f64,
    pub(crate) seconds: f64,
    pub(crate) speed: f64,
    pub(crate) priority: PriorityUi,
    pub(crate) fps: Option<u32>,
    pub(crate) side_px: Option<u32>,
    pub(crate) framing: FramingSpecUi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FramingModeUi {
    Crop,
    Fit,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub(crate) enum BackgroundUi {
    #[default]
    Transparent,
    Color {
        rgb: String,
    },
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShapeUi {
    #[default]
    Square,
    Rounded,
    Circle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlignUi {
    Top,
    #[default]
    Center,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) struct FramingSpecUi {
    pub(crate) mode: FramingModeUi,
    pub(crate) center_x: f64,
    pub(crate) center_y: f64,
    pub(crate) zoom: f64,
    #[serde(default)]
    pub(crate) background: BackgroundUi,
    #[serde(default)]
    pub(crate) shape: ShapeUi,
    #[serde(default = "full_scale")]
    pub(crate) scale: f64,
    #[serde(default)]
    pub(crate) align: AlignUi,
}

fn full_scale() -> f64 {
    1.0
}

impl Default for FramingSpecUi {
    fn default() -> Self {
        FramingSpecUi {
            mode: FramingModeUi::Crop,
            center_x: 0.5,
            center_y: 0.5,
            zoom: 1.0,
            background: BackgroundUi::default(),
            shape: ShapeUi::default(),
            scale: full_scale(),
            align: AlignUi::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) struct StaticRequest {
    pub(crate) id: u32,
    pub(crate) framing: FramingSpecUi,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct EstimateUi {
    pub(crate) estimated_size_bytes: u64,
    pub(crate) max_size_bytes: u64,
    pub(crate) fits: bool,
    pub(crate) fps: u32,
    pub(crate) max_seconds_s: f64,
    /// Longest range of the source video, in seconds, that fits the duration cap at the current
    /// speed and the size limit with the current options.
    pub(crate) max_range_s: f64,
    pub(crate) detail_px: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum ProgressUi {
    Decoding,
    Preview,
    FittingSize { attempt: u32, max: u32 },
    SearchingDuration { attempt: u32, max: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CreateResult {
    pub(crate) meets_limit: bool,
    pub(crate) size_bytes: u64,
    pub(crate) max_size_bytes: u64,
    pub(crate) fps: Option<u32>,
    pub(crate) duration_s: Option<f64>,
    pub(crate) reductions: Vec<String>,
    pub(crate) notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct LibraryUi {
    pub(crate) categories: Vec<CategoryUi>,
    pub(crate) packs: Vec<PackUi>,
    pub(crate) notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CategoryUi {
    pub(crate) id: u32,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct PackUi {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) category: Option<u32>,
    pub(crate) kind: Option<StickerKind>,
    pub(crate) stickers: Vec<StickerUi>,
    pub(crate) icon: Option<String>,
    pub(crate) exportable: bool,
    pub(crate) checks: Vec<CheckUi>,
    pub(crate) favorite: bool,
    pub(crate) modified: u64,
    pub(crate) published: Option<u32>,
    pub(crate) origin: OriginUi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckKey {
    Count,
    Kind,
    Emojis,
    Icon,
    Texts,
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CheckUi {
    pub(crate) key: CheckKey,
    pub(crate) ok: bool,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub(crate) enum FavoriteTarget {
    Pack { id: u32 },
    Sticker { pack: u32, id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CreatedPack {
    pub(crate) library: LibraryUi,
    pub(crate) pack: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub(crate) enum OriginUi {
    Own,
    Community {
        id: String,
        version: u32,
        license: String,
        original_author: String,
        withdrawn: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct StickerUi {
    pub(crate) id: u32,
    pub(crate) path: String,
    pub(crate) kind: StickerKind,
    pub(crate) size_bytes: u64,
    pub(crate) emojis: Vec<String>,
    pub(crate) favorite: bool,
    pub(crate) media_origin: MediaOrigin,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MediaOrigin {
    #[default]
    Own,
    Klipy,
    Giphy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StickerKind {
    Static,
    Animated,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) struct PackEdit {
    pub(crate) name: String,
    pub(crate) author: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) category: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ImportResult {
    pub(crate) library: LibraryUi,
    pub(crate) pack: u32,
    pub(crate) notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
#[error("{message}")]
pub(crate) struct UiError {
    pub(crate) kind: UiErrorKind,
    pub(crate) message: String,
}

impl UiError {
    pub(crate) fn new(kind: UiErrorKind, message: impl Into<String>) -> Self {
        UiError {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UiErrorKind {
    LinkNotValid,
    YtDlpNotFound,
    FailedDownload,
    FfmpegNotFound,
    InvalidEntry,
    InvalidOptions,
    DoesNotFitEvenSpedUp,
    TimedOut,
    Cancelled,
    Busy,
    Other,
    InvalidPack,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const EXAMPLES: &str = include_str!("../../src/contract.examples.json");

    fn example(name: &str) -> Value {
        let examples: Value = serde_json::from_str(EXAMPLES).unwrap();
        examples
            .get(name)
            .unwrap_or_else(|| panic!("falta el ejemplo {name}"))
            .clone()
    }

    fn example_error() -> UiError {
        UiError::new(UiErrorKind::FailedDownload, "yt-dlp falló (código 1)")
    }

    fn ready_entry() -> EntryUi {
        EntryUi {
            id: 1,
            name: "gato.mp4".to_string(),
            origin: Origin::File,
            state: EntryState::Ready,
            media: Some(MediaUi::Video {
                duration_s: 12.5,
                width: 1920,
                height: 1080,
                has_audio: true,
            }),
        }
    }

    #[test]
    fn preset_ui() {
        let preset = PresetUi {
            name: "whatsapp-sticker".to_string(),
            title: "Sticker animado de WhatsApp".to_string(),
            extension: "webp".to_string(),
            side: 512,
            max_size_bytes: 500_000,
            max_duration_s: 10.0,
            max_fps: 30,
            fps_range: FpsRangeUi { min: 5, max: 30 },
            side_range_px: None,
        };
        assert_eq!(serde_json::to_value(preset).unwrap(), example("preset_ui"));
    }

    #[test]
    fn preset_ui_with_free_side() {
        let preset = PresetUi {
            name: "discord-gif".to_string(),
            title: "GIF para Discord".to_string(),
            extension: "gif".to_string(),
            side: 480,
            max_size_bytes: 10_000_000,
            max_duration_s: 30.0,
            max_fps: 30,
            fps_range: FpsRangeUi { min: 5, max: 30 },
            side_range_px: Some(SideRangeUi {
                min: 64,
                max: 480,
                suggested: vec![240, 320, 480],
            }),
        };
        assert_eq!(
            serde_json::to_value(preset).unwrap(),
            example("preset_ui_free_side")
        );
    }

    #[test]
    fn entries_in_each_state() {
        assert_eq!(
            serde_json::to_value(ready_entry()).unwrap(),
            example("ready_entry")
        );
        let downloading = EntryUi {
            id: 2,
            name: "www.tiktok.com".to_string(),
            origin: Origin::Link,
            state: EntryState::Downloading {
                bytes: 1024,
                total: None,
            },
            media: None,
        };
        assert_eq!(
            serde_json::to_value(downloading).unwrap(),
            example("downloading_entry")
        );
        let pending = EntryUi {
            id: 3,
            name: "youtu.be".to_string(),
            origin: Origin::Link,
            state: EntryState::Pending,
            media: None,
        };
        assert_eq!(
            serde_json::to_value(pending).unwrap(),
            example("pending_entry")
        );
        let with_error = EntryUi {
            id: 4,
            name: "roto.mp4".to_string(),
            origin: Origin::File,
            state: EntryState::Error {
                error: example_error(),
            },
            media: None,
        };
        assert_eq!(
            serde_json::to_value(with_error).unwrap(),
            example("error_entry")
        );
    }

    #[test]
    fn image_has_path_and_video_does_not() {
        let image = EntryUi {
            id: 5,
            name: "gato.png".to_string(),
            origin: Origin::File,
            state: EntryState::Ready,
            media: Some(MediaUi::Image {
                path: "/home/ana/Imágenes/gato.png".to_string(),
                width: 800,
                height: 600,
            }),
        };
        assert_eq!(serde_json::to_value(image).unwrap(), example("image_entry"));
        let video = serde_json::to_value(ready_entry()).unwrap();
        assert!(video["media"].get("path").is_none());
    }

    #[test]
    fn queue_events() {
        let change = QueueEvent::Change {
            entry: ready_entry(),
        };
        assert_eq!(
            serde_json::to_value(change).unwrap(),
            example("change_event")
        );
        assert_eq!(
            serde_json::to_value(QueueEvent::Finished).unwrap(),
            example("finished_event")
        );
    }

    #[test]
    fn save_video_events() {
        let downloading = SaveVideoEvent::Downloading {
            bytes: 1024,
            total: None,
        };
        assert_eq!(
            serde_json::to_value(downloading).unwrap(),
            example("save_video_downloading_event")
        );
        assert_eq!(
            serde_json::to_value(SaveVideoEvent::Writing).unwrap(),
            example("save_video_writing_event")
        );
    }

    #[test]
    fn create_request_reads_from_interface_json() {
        let request: CreateRequest = serde_json::from_value(example("create_request")).unwrap();
        assert_eq!(
            request,
            CreateRequest {
                id: 1,
                preset: "discord-gif".to_string(),
                start_s: 3.2,
                seconds: 6.0,
                speed: 1.5,
                priority: PriorityUi::Smoothness,
                fps: Some(24),
                side_px: Some(320),
                framing: FramingSpecUi {
                    mode: FramingModeUi::Fit,
                    center_x: 0.25,
                    center_y: 0.5,
                    zoom: 2.0,
                    background: BackgroundUi::Blur,
                    shape: ShapeUi::Circle,
                    scale: 0.8,
                    align: AlignUi::Top,
                },
            }
        );
    }

    #[test]
    fn create_request_with_missing_or_extra_fields_is_rejected() {
        for field in [
            "id", "preset", "start_s", "seconds", "speed", "priority", "framing",
        ] {
            let mut incomplete = example("create_request");
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<CreateRequest>(incomplete).is_err(),
                "sin {field}"
            );
        }
        let mut with_fitting = example("create_request");
        with_fitting["fitting"] = json!(false);
        assert!(serde_json::from_value::<CreateRequest>(with_fitting).is_err());
        let old = json!({
            "id": 1, "preset": "discord-gif", "start_s": 0.0, "seconds": 1.0, "fitting": false
        });
        assert!(serde_json::from_value::<CreateRequest>(old).is_err());
        let mut with_extra = example("create_request");
        with_extra["path"] = json!("/etc/passwd");
        assert!(serde_json::from_value::<CreateRequest>(with_extra).is_err());
    }

    #[test]
    fn known_media_kinds_and_no_more() {
        let kinds: Vec<MediaKind> = serde_json::from_value(example("media_kinds")).unwrap();
        assert_eq!(kinds, [MediaKind::Image, MediaKind::Video]);
        assert!(serde_json::from_value::<MediaKind>(json!("audio")).is_err());
    }

    fn list<'a>(text: &'a str, from: &str, until: &str) -> Vec<&'a str> {
        let (_, rest) = text.split_once(from).unwrap();
        let (inside, _) = rest.split_once(until).unwrap();
        inside
            .lines()
            .map(|line| line.trim().trim_end_matches(','))
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .map(|line| line.trim_matches('"'))
            .collect()
    }

    fn declared_commands() -> Vec<&'static str> {
        list(
            include_str!("../build.rs"),
            "const COMMANDS: &[&str] = &[",
            "];",
        )
    }

    fn is_mobile_command(command: &&str) -> bool {
        [
            "add_to_whatsapp",
            "paste_from_clipboard",
            "preview_path",
            "receive_shared",
            "share_sticker",
            "share_result",
            "send_result_to_whatsapp",
            "send_sticker_to_whatsapp",
            "open_saved_folder",
        ]
        .contains(command)
    }

    #[test]
    fn every_command_is_registered_and_allowed() {
        let declared = declared_commands();
        let registered: Vec<_> = list(include_str!("lib.rs"), "generate_handler![", "])")
            .into_iter()
            .map(|path| path.rsplit("::").next().unwrap())
            .collect();
        assert_eq!(registered, declared);

        let (mobile, desktop): (Vec<_>, Vec<_>) = declared.into_iter().partition(is_mobile_command);
        assert_eq!(mobile.len(), 9);

        let capability: Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        let permissions: Vec<_> = desktop
            .iter()
            .map(|command| format!("allow-{}", command.replace('_', "-")))
            .collect();
        assert_eq!(capability["permissions"], json!(permissions));
    }

    #[test]
    fn every_desktop_command_is_in_the_interface() {
        let interface: String = ["contract.ts", "contract-community.ts", "contract-gifs.ts"]
            .iter()
            .map(|file| {
                let path = format!("{}/../src/{file}", env!("CARGO_MANIFEST_DIR"));
                std::fs::read_to_string(path).unwrap()
            })
            .collect();
        for command in declared_commands()
            .into_iter()
            .filter(|command| !is_mobile_command(command))
        {
            assert!(
                interface.contains(&format!("(\"{command}\"")),
                "{command} is not in contract.ts, contract-community.ts or contract-gifs.ts"
            );
        }
    }

    #[test]
    fn known_priorities_and_no_more() {
        let priorities: Vec<PriorityUi> = serde_json::from_value(example("priorities")).unwrap();
        assert_eq!(
            priorities,
            [
                PriorityUi::Sharpness,
                PriorityUi::Balance,
                PriorityUi::Smoothness,
                PriorityUi::FitAnyway,
                PriorityUi::Fluid
            ]
        );
        for text in ["maximum", "Sharpness", ""] {
            let mut request = example("create_request");
            request["priority"] = json!(text);
            assert!(
                serde_json::from_value::<CreateRequest>(request).is_err(),
                "{text}"
            );
        }
    }

    #[test]
    fn progress_at_each_stage() {
        let stages = [
            ProgressUi::Decoding,
            ProgressUi::Preview,
            ProgressUi::FittingSize { attempt: 2, max: 8 },
            ProgressUi::SearchingDuration { attempt: 1, max: 4 },
        ];
        assert_eq!(serde_json::to_value(stages).unwrap(), example("progresses"));
    }

    #[test]
    fn create_result() {
        let result = CreateResult {
            meets_limit: false,
            size_bytes: 612_000,
            max_size_bytes: 500_000,
            fps: Some(10),
            duration_s: Some(6.0),
            reductions: vec!["fps 15→10".to_string()],
            notice: Some("Prueba con un tramo de 4,5 s".to_string()),
        };
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            example("create_result")
        );
    }

    #[test]
    fn static_result_has_no_fps_or_duration() {
        let result = CreateResult {
            meets_limit: true,
            size_bytes: 84_000,
            max_size_bytes: 100_000,
            fps: None,
            duration_s: None,
            reductions: Vec::new(),
            notice: None,
        };
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            example("static_result")
        );
    }

    #[test]
    fn error_ui_and_all_its_kinds() {
        assert_eq!(
            serde_json::to_value(example_error()).unwrap(),
            example("error_ui")
        );
        let kinds = [
            UiErrorKind::LinkNotValid,
            UiErrorKind::YtDlpNotFound,
            UiErrorKind::FailedDownload,
            UiErrorKind::FfmpegNotFound,
            UiErrorKind::InvalidEntry,
            UiErrorKind::InvalidOptions,
            UiErrorKind::DoesNotFitEvenSpedUp,
            UiErrorKind::TimedOut,
            UiErrorKind::Cancelled,
            UiErrorKind::Busy,
            UiErrorKind::Other,
            UiErrorKind::InvalidPack,
        ];
        assert_eq!(serde_json::to_value(kinds).unwrap(), example("error_kinds"));
    }

    #[test]
    fn origins() {
        let origins = [Origin::File, Origin::Link, Origin::Pasted];
        assert_eq!(serde_json::to_value(origins).unwrap(), example("origins"));
    }

    #[test]
    fn static_request_with_known_framings_and_no_more() {
        let request: StaticRequest = serde_json::from_value(example("static_request")).unwrap();
        assert_eq!(
            request,
            StaticRequest {
                id: 5,
                framing: FramingSpecUi {
                    mode: FramingModeUi::Fit,
                    background: BackgroundUi::Color {
                        rgb: "#ffcc00".to_string()
                    },
                    shape: ShapeUi::Rounded,
                    align: AlignUi::Bottom,
                    ..FramingSpecUi::default()
                }
            }
        );
        let framings: Vec<FramingModeUi> = serde_json::from_value(example("framings")).unwrap();
        assert_eq!(framings, [FramingModeUi::Crop, FramingModeUi::Fit]);
        let whole = json!({ "mode": "crop", "center_x": 0.5, "center_y": 0.5, "zoom": 1.0 });
        // The style fields are optional: an old framing keeps meaning "square, full size".
        let old: StaticRequest =
            serde_json::from_value(json!({ "id": 5, "framing": whole })).unwrap();
        assert_eq!(old.framing, FramingSpecUi::default());
        let mut unknown_background = whole.clone();
        unknown_background["background"] = json!({ "kind": "gradient" });
        let mut unknown_shape = whole.clone();
        unknown_shape["shape"] = json!("star");
        let mut unknown_align = whole.clone();
        unknown_align["align"] = json!("left");
        let mut extra_field = whole.clone();
        extra_field["rotation"] = json!(90);
        for framing in [
            unknown_background,
            unknown_shape,
            unknown_align,
            extra_field,
        ] {
            assert!(
                serde_json::from_value::<StaticRequest>(json!({ "id": 5, "framing": framing }))
                    .is_err(),
                "{framing}"
            );
        }
        for request in [
            json!({ "id": 5, "framing": "crop" }),
            json!({ "id": 5, "framing": { "mode": "stretch", "center_x": 0.5, "center_y": 0.5, "zoom": 1.0 } }),
            json!({ "id": 5, "framing": { "mode": "crop", "center_x": 0.5, "center_y": 0.5 } }),
            json!({ "id": 5, "framing": whole, "path": "/etc/passwd" }),
            json!({ "id": 5 }),
        ] {
            assert!(
                serde_json::from_value::<StaticRequest>(request.clone()).is_err(),
                "{request}"
            );
        }
    }

    #[test]
    fn estimate_ui() {
        let estimate = EstimateUi {
            estimated_size_bytes: 380_000,
            max_size_bytes: 500_000,
            fits: true,
            fps: 15,
            max_seconds_s: 6.5,
            max_range_s: 6.5,
            detail_px: 384,
        };
        assert_eq!(
            serde_json::to_value(estimate).unwrap(),
            example("estimate_ui")
        );
    }

    #[test]
    fn library_ui() {
        let checks = || {
            vec![CheckUi {
                key: CheckKey::Count,
                ok: false,
                text: "necesita de 3 a 30 stickers".to_string(),
            }]
        };
        let library = LibraryUi {
            categories: vec![CategoryUi {
                id: 1,
                name: "Animales".to_string(),
            }],
            packs: vec![
                PackUi {
                    id: 3,
                    name: "Gatos".to_string(),
                    author: "Ana".to_string(),
                    category: Some(1),
                    kind: Some(StickerKind::Animated),
                    stickers: vec![StickerUi {
                        id: 5,
                        path: "/datos/packs/3/5.webp".to_string(),
                        kind: StickerKind::Animated,
                        size_bytes: 412_000,
                        emojis: vec!["🐱".to_string()],
                        favorite: true,
                        media_origin: MediaOrigin::Klipy,
                    }],
                    icon: Some("/datos/packs/3/tray.png".to_string()),
                    exportable: false,
                    checks: checks(),
                    favorite: true,
                    modified: 1_790_000_000,
                    published: Some(2),
                    origin: OriginUi::Community {
                        id: "gatos-garabato".to_string(),
                        version: 2,
                        license: "CC-BY-4.0".to_string(),
                        original_author: "Ana".to_string(),
                        withdrawn: None,
                    },
                },
                PackUi {
                    id: 4,
                    name: "Vacío".to_string(),
                    author: String::new(),
                    category: None,
                    kind: None,
                    stickers: Vec::new(),
                    icon: None,
                    exportable: false,
                    checks: checks(),
                    favorite: false,
                    modified: 1_790_000_100,
                    published: None,
                    origin: OriginUi::Own,
                },
            ],
            notice: Some("se quitó un sticker sin archivo".to_string()),
        };
        assert_eq!(
            serde_json::to_value(library).unwrap(),
            example("library_ui")
        );
        let kinds = [StickerKind::Static, StickerKind::Animated];
        assert_eq!(
            serde_json::to_value(kinds).unwrap(),
            example("sticker_kinds")
        );
    }

    #[test]
    fn favorite_target_and_created_pack() {
        let targets: Vec<FavoriteTarget> =
            serde_json::from_value(example("favorite_targets")).unwrap();
        assert_eq!(
            targets,
            [
                FavoriteTarget::Pack { id: 3 },
                FavoriteTarget::Sticker { pack: 3, id: 5 }
            ]
        );
        for target in [
            json!({ "kind": "sticker", "id": 5 }),
            json!({ "kind": "pack", "id": 3, "extra": 1 }),
            json!({ "kind": "other", "id": 3 }),
        ] {
            assert!(
                serde_json::from_value::<FavoriteTarget>(target.clone()).is_err(),
                "{target}"
            );
        }
        let created = CreatedPack {
            library: LibraryUi {
                categories: Vec::new(),
                packs: Vec::new(),
                notice: None,
            },
            pack: 9,
        };
        assert_eq!(
            serde_json::to_value(created).unwrap(),
            example("created_pack")
        );
    }

    #[test]
    fn pack_edit_always_states_the_category() {
        let edit: PackEdit = serde_json::from_value(example("pack_edit")).unwrap();
        assert_eq!(
            edit,
            PackEdit {
                name: "Gatos".to_string(),
                author: "Ana".to_string(),
                category: Some(1),
            }
        );
        let without: PackEdit =
            serde_json::from_value(example("pack_edit_without_category")).unwrap();
        assert_eq!(without.category, None);
        for field in ["name", "author", "category"] {
            let mut incomplete = example("pack_edit");
            incomplete.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PackEdit>(incomplete).is_err(),
                "sin {field}"
            );
        }
        let mut with_extra = example("pack_edit");
        with_extra["id"] = json!(3);
        assert!(serde_json::from_value::<PackEdit>(with_extra).is_err());
    }

    #[test]
    fn import_result() {
        let result = ImportResult {
            library: LibraryUi {
                categories: Vec::new(),
                packs: Vec::new(),
                notice: None,
            },
            pack: 7,
            notices: vec!["se omitió 1 sticker estático".to_string()],
        };
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            example("import_result")
        );
    }
}
