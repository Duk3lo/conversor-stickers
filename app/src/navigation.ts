export type Section = "stickers" | "gifs" | "download" | "search-gif";

export const Sections: readonly Section[] = ["stickers", "gifs", "download", "search-gif"];

export type Root = "start" | "community" | "create" | "favorites" | "mis-packs";

export const Roots: readonly Root[] = ["start", "community", "create", "favorites", "mis-packs"];

export type PathOrigin = "local" | "community";

export type Path =
  | { readonly kind: Root }
  | { readonly kind: "pack"; readonly origin: PathOrigin; readonly id: string }
  | { readonly kind: "category"; readonly origin: PathOrigin; readonly id: string }
  | { readonly kind: "search"; readonly query: string };

export type Destination = "sticker" | "gif";

export interface OpenEditor {
  readonly entry: number;
  readonly destination: Destination;
}

export interface Navigation {
  readonly path: Path;
  readonly stack: readonly Path[];
  readonly section: Section;
  readonly editor: OpenEditor | null;
  readonly openedPack: number | null;
}

const STACK_MAX = 20;

export function initialNavigation(): Navigation {
  return { path: { kind: "start" }, stack: [], section: "stickers", editor: null, openedPack: null };
}

export function rootEs(path: Path): boolean {
  return Roots.some((root) => root === path.kind);
}

export function pathToText(path: Path): string {
  switch (path.kind) {
    case "pack":
    case "category":
      return `${path.kind}/${path.origin}/${path.id}`;
    case "search":
      return `search/${path.query}`;
    default:
      return path.kind;
  }
}

export function textToPath(text: string): Path | null {
  const root = Roots.find((candidate) => candidate === text);
  if (root !== undefined) return { kind: root };
  const pieces = text.split("/");
  const [kind, origin] = pieces;
  if ((kind === "pack" || kind === "category") && (origin === "local" || origin === "community")) {
    const id = pieces.slice(2).join("/");
    return id === "" ? null : { kind, origin, id };
  }
  if (kind === "search") return { kind: "search", query: pieces.slice(1).join("/") };
  return null;
}

function pathSame(a: Path, b: Path): boolean {
  return pathToText(a) === pathToText(b);
}

export function goToRoot(nav: Navigation, root: Root): Navigation {
  return { ...nav, path: { kind: root }, stack: [], editor: null };
}

export function goTo(nav: Navigation, path: Path): Navigation {
  if (nav.editor === null && pathSame(nav.path, path)) return nav;
  return { ...nav, path, stack: [...nav.stack, nav.path].slice(-STACK_MAX), editor: null };
}

export function replace(nav: Navigation, path: Path): Navigation {
  return { ...nav, path, editor: null };
}

export function changeSection(nav: Navigation, section: Section): Navigation {
  return { ...goToRoot(nav, "create"), section };
}

export function openEditor(nav: Navigation, entry: number, destination: Destination): Navigation {
  return { ...nav, editor: { entry, destination } };
}

export function closeEditor(nav: Navigation): Navigation {
  return nav.editor === null ? nav : { ...nav, editor: null };
}

export function openPack(nav: Navigation, id: number): Navigation {
  return { ...goToRoot(nav, "mis-packs"), openedPack: id };
}

export function closePack(nav: Navigation): Navigation {
  return { ...nav, openedPack: null };
}

export function back(nav: Navigation): Navigation | null {
  if (nav.editor !== null) return closeEditor(nav);
  if (nav.path.kind === "mis-packs" && nav.openedPack !== null) return closePack(nav);
  const previous = nav.stack[nav.stack.length - 1];
  if (previous === undefined) return null;
  return { ...nav, path: previous, stack: nav.stack.slice(0, -1) };
}

export function withPacks(nav: Navigation, ids: readonly number[]): Navigation {
  let result = nav;
  if (result.openedPack !== null && !ids.includes(result.openedPack)) {
    result = closePack(result);
  }
  const path = result.path;
  if (path.kind === "pack" && path.origin === "local" && !ids.some((id) => String(id) === path.id)) {
    const previous = result.stack[result.stack.length - 1];
    result = {
      ...result,
      path: previous ?? { kind: "mis-packs" },
      stack: result.stack.slice(0, -1),
    };
  }
  return result;
}

export function withoutEntry(nav: Navigation, id: number): Navigation {
  return nav.editor?.entry === id ? closeEditor(nav) : nav;
}

export function activeRoot(nav: Navigation): Root {
  const origin = nav.stack[0] ?? nav.path;
  return Roots.find((root) => root === origin.kind) ?? "start";
}

export type PanelVisible = Root | "detail" | "category" | "search" | "editor";

export function panelVisible(nav: Navigation): PanelVisible {
  if (nav.editor !== null) return "editor";
  switch (nav.path.kind) {
    case "pack":
      return "detail";
    default:
      return nav.path.kind;
  }
}

export function sectionWithKey(current: Section, key: string): Section | null {
  const index = Sections.indexOf(current);
  const total = Sections.length;
  switch (key) {
    case "ArrowRight":
      return Sections[(index + 1) % total] ?? null;
    case "ArrowLeft":
      return Sections[(index - 1 + total) % total] ?? null;
    case "Home":
      return Sections[0] ?? null;
    case "End":
      return Sections[total - 1] ?? null;
    default:
      return null;
  }
}
