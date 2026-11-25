import type { LibraryUi } from "./contract.ts";
import type { CatalogUi } from "./contract-community.ts";
import { communityCard, normalize, type PackCard } from "./catalog.ts";
import type { Path } from "./navigation.ts";

export const FEATURED_MAX = 6;

export type CommunitySort = "recent" | "az";

export interface CommunityEntry {
  readonly card: PackCard;
  readonly license: string;
  readonly downloaded: number | null;
  readonly updatable: boolean;
  readonly withdrawn: boolean;
}

export interface CommunityCriteria {
  readonly query: string;
  readonly filter: string;
  readonly sort: CommunitySort;
}

export interface EntryAction {
  readonly kind: "download" | "open" | "update";
  readonly text: string;
  readonly path: Path;
}

const FixedFilters = [
  { key: "all", name: "Todos" },
  { key: "static", name: "Estáticos" },
  { key: "animated", name: "Animados" },
  { key: "downloaded", name: "Descargados" },
] as const;

export function communityEntries(catalog: CatalogUi, library: LibraryUi | null): CommunityEntry[] {
  return catalog.packs.map((pack) => {
    const backup = library?.packs.find((local) => local.id === pack.downloaded) ?? null;
    const card = communityCard(pack, catalog.categories, backup);
    return {
      card,
      license: pack.license,
      downloaded: pack.downloaded,
      updatable: pack.updatable,
      withdrawn: card.marks.includes("Retirado"),
    };
  });
}

export function filterChips(catalog: CatalogUi): { key: string; name: string }[] {
  const present = new Set(catalog.packs.map((pack) => pack.category));
  const categories = catalog.categories
    .filter((category) => present.has(category.id))
    .map((category) => ({ key: `category:${category.id}`, name: category.name }));
  return [...FixedFilters, ...categories];
}

function matchesFilter(entry: CommunityEntry, filter: string): boolean {
  switch (filter) {
    case "all":
      return true;
    case "static":
      return entry.card.animated === false;
    case "animated":
      return entry.card.animated === true;
    case "downloaded":
      return entry.downloaded !== null;
    default:
      return filter.startsWith("category:") && entry.card.idCategory === filter.slice("category:".length);
  }
}

function matchesQuery(entry: CommunityEntry, words: readonly string[]): boolean {
  const text = normalize(
    [entry.card.name, entry.card.author, entry.card.category ?? "", entry.license, ...entry.card.tags].join(" "),
  );
  return words.every((word) => text.includes(word));
}

export function compareEntries(sort: CommunitySort): (a: CommunityEntry, b: CommunityEntry) => number {
  const byName = (a: CommunityEntry, b: CommunityEntry): number => a.card.name.localeCompare(b.card.name, "es");
  return sort === "az" ? byName : (a, b) => b.card.added.localeCompare(a.card.added) || byName(a, b);
}

export function filterEntries(entries: readonly CommunityEntry[], criteria: CommunityCriteria): CommunityEntry[] {
  const words = normalize(criteria.query).split(/\s+/).filter((word) => word !== "");
  return entries
    .filter((entry) => matchesFilter(entry, criteria.filter) && matchesQuery(entry, words))
    .toSorted(compareEntries(criteria.sort));
}

export function featuredEntries(entries: readonly CommunityEntry[], max = FEATURED_MAX): CommunityEntry[] {
  return entries.toSorted(compareEntries("recent")).slice(0, max);
}

export function entryAction(entry: CommunityEntry): EntryAction {
  const community: Path = { kind: "pack", origin: "community", id: entry.card.id };
  if (entry.downloaded === null) return { kind: "download", text: "Descargar", path: community };
  if (entry.updatable) return { kind: "update", text: "Actualizar", path: community };
  return { kind: "open", text: "Abrir", path: { kind: "pack", origin: "local", id: String(entry.downloaded) } };
}

export function packsCountText(count: number): string {
  return count === 1 ? "1 paquete" : `${count} paquetes`;
}
