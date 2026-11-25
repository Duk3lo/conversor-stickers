import type { SavedFolder } from "./saved-folder.ts";
import type { ExplorerModel, PackCard } from "./catalog.ts";
import type { FavoritesModel, SearchResult, StickerResult } from "./search.ts";
import type { ActionKey, DetailModel } from "./detail.ts";
import type { GifSearch } from "./gif-search.ts";
import type { CommunityProgressUi } from "./contract-community.ts";
import type { GifProvider, GifProviderUi, StickerKlipyUi } from "./contract-gifs.ts";
import type { KlipyRow } from "./klipy.ts";
import type { Queue } from "./queue.ts";
import type { Saving } from "./saving.ts";
import type { ClipVideoState } from "./clip-video.ts";
import type { SoundState } from "./sound.ts";
import type {
  LibraryUi,
  CategoryUi,
  PackEdit,
  FramingSpecUi,
  UiError,
  EstimateUi,
  MediaUi,
  PresetUi,
  PriorityUi,
  ProgressUi,
  CreateResult,
  MediaKind,
} from "./contract.ts";
import type { Destination, Navigation, Root, Path, Section } from "./navigation.ts";
import type { PreviewState } from "./preview.ts";
import type { ClipRange } from "./crop.ts";
import type { EditorMode } from "./easy.ts";

export type Load<T> =
  | { readonly kind: "loading" }
  | { readonly kind: "ready"; readonly value: T }
  | { readonly kind: "error"; readonly message: string };

export interface ShellView {
  readonly nav: Navigation;
  readonly notice: string | null;
  readonly noticeFolder: SavedFolder | null;
}

export interface ShellActions {
  goToRoot(root: Root): void;
  goToCreate(): void;
  changeSection(section: Section): void;
  closeNotice(): void;
  openFolder(folder: SavedFolder): void;
}

export type CategoryFilter =
  | { readonly kind: "all" }
  | { readonly kind: "without-category" }
  | { readonly kind: "category"; readonly id: number };

export type StickersForm =
  | { readonly kind: "category-new" }
  | { readonly kind: "rename-category"; readonly id: number }
  | { readonly kind: "pack-new" };

export type Confirmation =
  | { readonly kind: "delete-pack"; readonly id: number }
  | { readonly kind: "delete-category"; readonly id: number };

export interface StickersView {
  readonly library: Load<LibraryUi>;
  readonly filter: CategoryFilter;
  readonly openedPack: number | null;
  readonly form: StickersForm | null;
  readonly formError: string | null;
  readonly confirmation: Confirmation | null;
  readonly chosenSticker: number | null;
  readonly busy: boolean;
  readonly choosing: MediaKind | null;
  readonly error: string | null;
  readonly notices: readonly string[];
}

export interface StickersActions {
  reload(): void;
  pickFilter(filter: CategoryFilter): void;
  openForm(form: StickersForm): void;
  closeForm(): void;
  sendForm(text: string): void;
  requestConfirmation(confirmation: Confirmation): void;
  cancelConfirmation(): void;
  confirm(): void;
  openPack(id: number): void;
  closePack(): void;
  savePack(edit: PackEdit): void;
  pickSticker(id: number | null): void;
  removeSticker(): void;
  useAsIcon(): void;
  doExport(): void;
  addToWhatsapp(): void;
  doImport(): void;
  createFromGallery(kind: MediaKind): void;
  closeNotices(): void;
}

export type Creation =
  | { readonly kind: "inactive" }
  | { readonly kind: "creating"; readonly progress: ProgressUi | null; readonly cancelling: boolean }
  | {
      readonly kind: "done";
      readonly result: CreateResult;
      readonly image: string | null;
      readonly saving: boolean;
      readonly saved: string | null;
      readonly saveError: string | null;
      readonly sentNotice: string | null;
      readonly adding: boolean;
      readonly addedTo: string | null;
      readonly addError: string | null;
    }
  | { readonly kind: "error"; readonly error: UiError };

export type EstimateState =
  | { readonly kind: "without-request" }
  | { readonly kind: "calculating" }
  | { readonly kind: "ready"; readonly estimate: EstimateUi }
  | { readonly kind: "failed"; readonly message: string };

export interface DestinationPack {
  readonly id: number;
  readonly name: string;
}

export interface EditorView {
  readonly entry: { readonly id: number; readonly name: string; readonly media: MediaUi };
  readonly destination: Destination;
  readonly presets: readonly PresetUi[];
  readonly preset: PresetUi | null;
  readonly preview: PreviewState;
  readonly image: string | null;
  readonly duration: number | null;
  readonly clipRange: ClipRange | null;
  readonly speed: number;
  readonly fitting: string | null;
  readonly priority: PriorityUi;
  readonly fps: number | null;
  readonly framing: FramingSpecUi;
  readonly sidePx: number | null;
  /** Small frames spread over the whole video, for the clip bar. */
  readonly thumbnails: readonly string[];
  readonly fitPending: boolean;
  readonly mode: EditorMode;
  /** «Crear sticker» of the easy mode is adjusting and about to create. */
  readonly preparing: boolean;
  /** One line about what the easy mode did or reset. */
  readonly easyNote: string | null;
  readonly estimate: EstimateState;
  readonly creation: Creation;
  readonly sound: SoundState;
  readonly clipVideo: ClipVideoState;
  readonly packs: {
    readonly open: DestinationPack | null;
    readonly compatible: readonly DestinationPack[];
  };
}

export interface EditorActions {
  back(): void;
  pickPreset(name: string): void;
  moveStart(seconds: number): void;
  moveEnd(seconds: number): void;
  moveRange(startS: number): void;
  fitToTime(): void;
  pickSide(side: number | null): void;
  changeSpeed(speed: number): void;
  fitMax(): void;
  pickPriority(priority: PriorityUi): void;
  pickFps(fps: number | null): void;
  setFraming(framing: FramingSpecUi): void;
  makeFit(): void;
  pickMode(mode: EditorMode): void;
  create(): void;
  cancelCreation(): void;
  save(): void;
  saveSound(): void;
  saveClipVideo(): void;
  openFolder(folder: SavedFolder): void;
  openWhatsapp(): void;
  share(): void;
  sendToWhatsapp(): void;
  addPack(id: number): void;
}

export interface CreatedGif {
  readonly entry: number;
  readonly name: string;
  readonly destination: string;
  readonly sizeBytes: number;
  readonly image: string;
  readonly saving: boolean;
  readonly saved: string | null;
  readonly saveError: string | null;
}

export interface GifsView {
  readonly choosing: boolean;
  readonly error: string | null;
  readonly created: readonly CreatedGif[];
}

export interface GifsActions {
  pickVideo(): void;
  save(entry: number): void;
}

export interface GifSearchView {
  readonly provider: Load<GifProviderUi>;
  readonly settings: boolean;
  readonly savingSettings: boolean;
  readonly settingsError: string | null;
  readonly text: string;
  readonly search: GifSearch;
  readonly using: string | null;
  readonly useError: string | null;
}

export interface GifSearchActions {
  toggleSettings(): void;
  saveSettings(provider: GifProvider, key: string | null): void;
  write(text: string): void;
  search(text: string): void;
  loadPlus(): void;
  retry(): void;
  use(id: string): void;
}

export type Pasted =
  | { readonly kind: "inactive" }
  | { readonly kind: "pasting" }
  | { readonly kind: "notice"; readonly text: string }
  | { readonly kind: "error"; readonly text: string };

export interface DownloadView {
  readonly queue: Queue;
  readonly sendingLinks: boolean;
  readonly linksError: string | null;
  readonly queueError: string | null;
  readonly pasted: Pasted;
  readonly saving: Saving | null;
}

export interface DownloadActions {
  addLinks(text: string): Promise<boolean>;
  cancelDownloads(): void;
  resumeDownloads(): void;
  remove(id: number): void;
  makeSticker(id: number): void;
  makeGif(id: number): void;
  saveVideo(id: number): void;
  cancelSaving(): void;
  openFolder(folder: SavedFolder): void;
  paste(): void;
}

export interface KlipyActions {
  openKlipySticker(sticker: StickerKlipyUi): void;
  plusSeeKlipy(key: string): void;
}

export interface KlipyStickerView {
  readonly sticker: StickerKlipyUi;
  readonly packs: readonly DestinationPack[];
  readonly busy: boolean;
  readonly error: string | null;
}

export interface KlipyStickerActions {
  close(): void;
  add(choice: string, newName: string): void;
}

export interface ExplorerView {
  readonly state: Load<ExplorerModel>;
  readonly catalogError: string | null;
  readonly withoutConnection: string | null;
  readonly history: readonly string[];
  readonly klipy: readonly KlipyRow[];
}

export interface ExplorerActions extends KlipyActions {
  pickChip(key: string | null): void;
  openPack(path: Path): void;
  seeAll(path: Path): void;
  search(text: string): void;
  update(): void;
  goToCreate(): void;
  goToCommunity(): void;
}

export interface DetailView {
  readonly state: Load<DetailModel>;
  readonly categories: readonly CategoryUi[];
  readonly busy: string | null;
  readonly progress: CommunityProgressUi | null;
  readonly error: string | null;
  readonly notice: string | null;
  readonly reordering: boolean;
  readonly openSticker: number | null;
  readonly confirmingDelete: boolean;
  readonly focus: string | null;
  readonly communityNotice: boolean;
  readonly proposing: boolean;
  readonly communityCategories: readonly { readonly id: string; readonly name: string }[];
  readonly proposeReason: string | null;
}

export interface DetailActions {
  back(): void;
  retry(): void;
  action(key: ActionKey): void;
  toggleFavorite(): void;
  toggleReorder(): void;
  moveSticker(index: number, delta: -1 | 1): void;
  dragSticker(from: number, until: number): void;
  changeCategory(category: number | null): void;
  requestDelete(): void;
  cancelDelete(): void;
  confirmDelete(): void;
  openSticker(index: number | null): void;
  toggleStickerFavorite(): void;
  removeSticker(): void;
  saveSticker(): void;
  shareSticker(): void;
  sendStickerToWhatsapp(): void;
  cancelDownload(): void;
  editEmojis(): void;
  acceptNotice(): void;
  cancelNotice(): void;
  openPropose(): void;
  closePropose(): void;
  propose(license: string, category: string): void;
}

export interface CategoryView {
  readonly state: Load<{ readonly name: string; readonly cards: readonly PackCard[] }>;
}

export interface CategoryActions {
  back(): void;
  openPack(path: Path): void;
}

export interface SearchView {
  readonly query: string;
  readonly history: readonly string[];
  readonly state: Load<SearchResult>;
  readonly klipy: KlipyRow | null;
}

export interface SearchActions extends KlipyActions {
  back(): void;
  search(text: string): void;
  removeFromHistory(text: string): void;
  openPack(path: Path): void;
  openSticker(sticker: StickerResult): void;
}

export interface FavoritesView {
  readonly state: Load<FavoritesModel>;
  readonly form: boolean;
  readonly formError: string | null;
  readonly busy: boolean;
  readonly error: string | null;
}

export interface FavoritesActions {
  openPack(path: Path): void;
  openSticker(sticker: StickerResult): void;
  openForm(): void;
  closeForm(): void;
  createPack(name: string): void;
  goToStart(): void;
}

export interface BackupView {
  readonly busy: string | null;
  readonly confirming: boolean;
  readonly notice: string | null;
  readonly error: string | null;
}

export interface BackupActions {
  doExport(): void;
  requestRestore(): void;
  cancelRestore(): void;
  restore(): void;
}
