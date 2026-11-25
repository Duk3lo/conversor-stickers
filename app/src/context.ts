import type { EntryUi } from "./contract.ts";
import type { Navigation } from "./navigation.ts";
import type { SavedFolder } from "./saved-folder.ts";

export interface Context {
  nav(): Navigation;
  navigate(fresh: Navigation): void;
  /** `folder`: shows «Abrir carpeta» next to the notice, for a save that worked. */
  notify(text: string | null, folder?: SavedFolder): void;
  openFolder(folder: SavedFolder): void;
  entry(id: number): EntryUi | undefined;
  remember(entry: EntryUi): void;
  forget(id: number): void;
  registerLayer(close: () => boolean): void;
  closeLayer(): boolean;
}
