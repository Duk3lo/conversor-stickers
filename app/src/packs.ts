import type { LibraryUi, CategoryUi, PackUi, StickerKind } from "./contract.ts";
import type { CategoryFilter, DestinationPack } from "./views.ts";

export const MAX_STICKERS = 30;
export const EXPORT_STICKERS_MIN = 3;
const PACK_NAME_MAX = 128;
const AUTHOR_MAX = 128;
const CATEGORY_NAME_MAX = 40;
const CATEGORIES_MAX = 50;

export type Validation = { readonly valid: true; readonly value: string } | { readonly valid: false; readonly reason: string };

export function filterPacks(library: LibraryUi, filter: CategoryFilter): readonly PackUi[] {
  switch (filter.kind) {
    case "all":
      return library.packs;
    case "without-category":
      return library.packs.filter((pack) => pack.category === null);
    case "category":
      return library.packs.filter((pack) => pack.category === filter.id);
  }
}

export function currentFilter(filter: CategoryFilter, library: LibraryUi): CategoryFilter {
  if (filter.kind !== "category") return filter;
  return library.categories.some((category) => category.id === filter.id) ? filter : { kind: "all" };
}

export function categoryForNew(filter: CategoryFilter): number | null {
  return filter.kind === "category" ? filter.id : null;
}

export function countText(pack: PackUi): string {
  return `${pack.stickers.length}/${MAX_STICKERS}`;
}

export function kindText(kind: StickerKind | null): string {
  switch (kind) {
    case null:
      return "Vacío";
    case "animated":
      return "Animado";
    case "static":
      return "Estático";
  }
}

export function exportableNoReason(pack: PackUi): string | null {
  if (pack.exportable) return null;
  const failure = pack.checks.find((check) => !check.ok);
  if (failure !== undefined) return failure.text;
  const missing = EXPORT_STICKERS_MIN - pack.stickers.length;
  if (missing > 0) {
    const howMany = missing === 1 ? "Falta 1 sticker" : `Faltan ${missing} stickers`;
    return `${howMany}: un paquete lleva de ${EXPORT_STICKERS_MIN} a ${MAX_STICKERS}.`;
  }
  return "Falta el icono de la bandeja.";
}

export function compatiblePacks(library: LibraryUi, kind: StickerKind): DestinationPack[] {
  return library.packs
    .filter((pack) => (pack.kind === null || pack.kind === kind) && pack.stickers.length < MAX_STICKERS)
    .map(({ id, name }) => ({ id, name }));
}

const CONTROL = /\p{Cc}/u;

function validateText(text: string, min: number, max: number, que: string): Validation {
  const value = text.trim();
  const long = [...value].length;
  if (long < min) return { valid: false, reason: "Escribe un nombre." };
  if (long > max) return { valid: false, reason: `${que} puede tener como mucho ${max} caracteres.` };
  if (CONTROL.test(value)) return { valid: false, reason: `${que} no puede llevar caracteres de control.` };
  return { valid: true, value };
}

export function validatePackName(name: string): Validation {
  return validateText(name, 1, PACK_NAME_MAX, "El nombre");
}

export function validateAuthor(author: string): Validation {
  return validateText(author, 0, AUTHOR_MAX, "El autor");
}

export function validateCategoryName(
  name: string,
  categories: readonly CategoryUi[],
  own: number | null,
): Validation {
  if (own === null && categories.length >= CATEGORIES_MAX) {
    return { valid: false, reason: `Ya tienes ${CATEGORIES_MAX} categorías, el máximo.` };
  }
  const validation = validateText(name, 1, CATEGORY_NAME_MAX, "El nombre");
  if (!validation.valid) return validation;
  const equal = categories.find(
    (category) => category.id !== own && category.name.toLowerCase() === validation.value.toLowerCase(),
  );
  if (equal) return { valid: false, reason: `Ya hay una categoría «${equal.name}».` };
  return validation;
}
