import type { LibraryUi, CategoryUi, PackUi } from "./contract.ts";
import type { CatalogUi, CommunityCategoryUi, CommunityPackUi } from "./contract-community.ts";
import type { PathOrigin, Path } from "./navigation.ts";

const SAMPLES_MAX = 4;
export const EDITING_FOLLOW_MAX = 8;
export const WITHOUT_CATEGORY = "without-category";

export interface PackCard {
  readonly key: string;
  readonly origin: PathOrigin;
  readonly id: string;
  readonly name: string;
  readonly author: string;
  readonly icon: string | null;
  readonly samples: readonly string[];
  readonly stickers: number;
  readonly animated: boolean | null;
  readonly category: string | null;
  readonly idCategory: string | null;
  readonly favorite: boolean;
  readonly marks: readonly string[];
  readonly tags: readonly string[];
  readonly modified: number;
  readonly added: string;
}

export interface ExplorerModel {
  readonly follow: readonly PackCard[];
  readonly chips: readonly { readonly key: string; readonly name: string }[];
  readonly chip: string | null;
  readonly rows: readonly CategoryRow[];
}

export interface CategoryRow {
  readonly key: string;
  readonly name: string;
  readonly cards: readonly PackCard[];
  readonly path: Path;
}

export function normalize(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .trim();
}

export function pathOfCard(card: PackCard): Path {
  return { kind: "pack", origin: card.origin, id: card.id };
}

export function localCard(pack: PackUi, categories: readonly CategoryUi[]): PackCard {
  const category = categories.find((candidate) => candidate.id === pack.category) ?? null;
  const marks: string[] = [];
  if (pack.origin.kind === "community") marks.push("Comunidad");
  if (pack.origin.kind === "community" && pack.origin.withdrawn !== null) marks.push("Retirado");
  return {
    key: `local/${pack.id}`,
    origin: "local",
    id: String(pack.id),
    name: pack.name,
    author: pack.origin.kind === "community" ? pack.origin.original_author : pack.author,
    icon: pack.icon,
    samples: pack.stickers.slice(0, SAMPLES_MAX).map((sticker) => sticker.path),
    stickers: pack.stickers.length,
    animated: pack.kind === null ? null : pack.kind === "animated",
    category: category?.name ?? null,
    idCategory: category === null ? null : String(category.id),
    favorite: pack.favorite,
    marks,
    tags: [],
    modified: pack.modified,
    added: "",
  };
}

export function communityCard(
  pack: CommunityPackUi,
  categories: readonly CommunityCategoryUi[],
  backup: PackUi | null,
): PackCard {
  const category = categories.find((candidate) => candidate.id === pack.category) ?? null;
  const marks = ["Comunidad"];
  if (pack.downloaded !== null) marks.push("Descargado");
  if (pack.updatable) marks.push("Actualización");
  if (backup?.origin.kind === "community" && backup.origin.withdrawn !== null) marks.push("Retirado");
  return {
    key: `community/${pack.id}`,
    origin: "community",
    id: pack.id,
    name: pack.name,
    author: pack.author,
    icon: pack.icon,
    samples: [],
    stickers: pack.stickers,
    animated: pack.animated,
    category: category?.name ?? null,
    idCategory: category?.id ?? null,
    favorite: backup?.favorite ?? false,
    marks,
    tags: pack.tags,
    modified: 0,
    added: pack.added,
  };
}

export function cards(library: LibraryUi | null, catalog: CatalogUi | null): PackCard[] {
  const locales = library?.packs ?? [];
  const downloaded = new Set(catalog?.packs.flatMap((pack) => (pack.downloaded === null ? [] : [pack.downloaded])));
  const own = locales
    .filter((pack) => !downloaded.has(pack.id))
    .map((pack) => localCard(pack, library?.categories ?? []));
  const remote = (catalog?.packs ?? []).map((pack) =>
    communityCard(
      pack,
      catalog?.categories ?? [],
      locales.find((local) => local.id === pack.downloaded) ?? null,
    ),
  );
  return [...own, ...remote];
}

export function rowsByCategory(list: readonly PackCard[], order: readonly string[] = []): CategoryRow[] {
  const rows = new Map<string, { name: string; cards: PackCard[]; path: Path }>();
  for (const card of list) {
    const key = card.category === null ? WITHOUT_CATEGORY : normalize(card.category);
    let row = rows.get(key);
    if (row === undefined) {
      const id = card.idCategory ?? "sin";
      row = {
        name: card.category ?? "Sin categoría",
        cards: [],
        path: { kind: "category", origin: card.origin, id },
      };
      rows.set(key, row);
    }
    row.cards.push(card);
  }
  const result = [...rows].map(([key, row]) => ({ key, ...row }));
  const position = (key: string): number => {
    const index = order.indexOf(key);
    return index === -1 ? order.length : index;
  };
  return result.sort((a, b) => {
    if (a.key === WITHOUT_CATEGORY || b.key === WITHOUT_CATEGORY) return Number(a.key === WITHOUT_CATEGORY) - Number(b.key === WITHOUT_CATEGORY);
    return position(a.key) - position(b.key);
  });
}

export function followEditing(library: LibraryUi | null, max = EDITING_FOLLOW_MAX): PackCard[] {
  if (library === null) return [];
  return library.packs
    .filter((pack) => pack.origin.kind === "own")
    .toSorted((a, b) => b.modified - a.modified || a.name.localeCompare(b.name, "es"))
    .slice(0, max)
    .map((pack) => localCard(pack, library.categories));
}

export function rowsWithChip(rows: readonly CategoryRow[], chip: string | null): readonly CategoryRow[] {
  return chip === null ? rows : rows.filter((row) => row.key === chip);
}

export function currentChip(chip: string | null, rows: readonly CategoryRow[]): string | null {
  return chip !== null && rows.some((row) => row.key === chip) ? chip : null;
}

export function keyByPath(
  origin: PathOrigin,
  id: string,
  library: LibraryUi | null,
  catalog: CatalogUi | null,
): { key: string; name: string } | null {
  if (id === "sin") return { key: WITHOUT_CATEGORY, name: "Sin categoría" };
  const name =
    origin === "local"
      ? library?.categories.find((category) => String(category.id) === id)?.name
      : catalog?.categories.find((category) => category.id === id)?.name;
  return name === undefined ? null : { key: normalize(name), name };
}

export function cardsOfCategory(list: readonly PackCard[], key: string): PackCard[] {
  return list.filter((card) => (card.category === null ? WITHOUT_CATEGORY : normalize(card.category)) === key);
}

export function updatedText(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString("es", { day: "numeric", month: "short", year: "numeric" });
}

function modelWithRows(all: readonly PackCard[], order: readonly string[], chip: string | null) {
  const rows = rowsByCategory(all, order);
  const current = currentChip(chip, rows);
  return {
    chips: rows.map(({ key, name }) => ({ key, name })),
    chip: current,
    rows: rowsWithChip(rows, current),
  };
}

export function startModel(library: LibraryUi, catalog: CatalogUi | null, chip: string | null): ExplorerModel {
  const order = library.categories.map((category) => normalize(category.name));
  return {
    follow: followEditing(library),
    ...modelWithRows(cards(library, catalog), order, chip),
  };
}
