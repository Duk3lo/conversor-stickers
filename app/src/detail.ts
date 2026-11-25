import type { LibraryUi, CheckUi, MediaOrigin, PackUi } from "./contract.ts";
import type { CommunityDetailUi, CommunityPackUi } from "./contract-community.ts";
import type { Platform } from "./mobile.ts";

export type ActionKey =
  | "add-whatsapp"
  | "update-whatsapp"
  | "export"
  | "download"
  | "update"
  | "open-backup";

export interface BarButton {
  readonly key: ActionKey;
  readonly text: string;
  readonly active: boolean;
}

export interface ActionBar {
  readonly main: BarButton;
  readonly secondary: BarButton | null;
  readonly state: string | null;
  readonly reason: string | null;
}

export interface DetailSticker {
  readonly id: number | null;
  readonly path: string;
  readonly emojis: readonly string[];
  readonly favorite: boolean;
  readonly mediaOrigin?: MediaOrigin;
}

export interface DetailModel {
  readonly origin: "local" | "community";
  readonly pack: number | null;
  readonly name: string;
  readonly author: string;
  readonly icon: string | null;
  readonly count: number;
  readonly animated: boolean | null;
  readonly license: string | null;
  readonly category: number | null;
  readonly favorite: boolean | null;
  readonly stickers: readonly DetailSticker[];
  readonly checks: readonly CheckUi[];
  readonly bar: ActionBar;
  readonly withdrawn: string | null;
  readonly version: string | null;
  readonly communityId: string | null;
}

function withdrawnReason(pack: PackUi): string | null {
  return pack.origin.kind === "community" ? pack.origin.withdrawn : null;
}

export function lockReason(pack: PackUi): string | null {
  const withdrawn = withdrawnReason(pack);
  if (withdrawn !== null) return withdrawn;
  return pack.checks.find((check) => !check.ok)?.text ?? (pack.exportable ? null : "El paquete no se puede publicar todavía");
}

function currentVersion(pack: PackUi): number | null {
  return pack.origin.kind === "community" ? pack.origin.version : null;
}

export function localBar(pack: PackUi, platform: Platform): ActionBar {
  const reason = lockReason(pack);
  const active = reason === null && pack.exportable;
  if (platform === "desktop") {
    return {
      main: { key: "export", text: "Exportar .wastickers", active },
      secondary: null,
      state: null,
      reason,
    };
  }
  const version = currentVersion(pack);
  const expired = pack.published !== null && version !== null && version > pack.published;
  return {
    main: expired
      ? { key: "update-whatsapp", text: "Actualizar en WhatsApp", active }
      : { key: "add-whatsapp", text: "Añadir a WhatsApp", active },
    secondary: null,
    state: pack.published !== null && !expired ? "Añadido a WhatsApp" : null,
    reason,
  };
}

export function communityBar(
  pack: CommunityPackUi,
  backup: PackUi | null,
  downloading: boolean,
): ActionBar {
  if (downloading) {
    return { main: { key: "download", text: "Descargando…", active: false }, secondary: null, state: null, reason: null };
  }
  if (pack.downloaded === null) {
    return { main: { key: "download", text: "Descargar", active: true }, secondary: null, state: null, reason: null };
  }
  const withdrawn = backup === null ? null : withdrawnReason(backup);
  if (withdrawn !== null) {
    return {
      main: { key: "open-backup", text: "Abrir mi copia", active: true },
      secondary: null,
      state: "Retirado del catálogo",
      reason: withdrawn,
    };
  }
  if (pack.updatable) {
    return {
      main: { key: "update", text: `Actualizar (v${pack.version})`, active: true },
      secondary: { key: "open-backup", text: "Abrir mi copia", active: true },
      state: "Descargado",
      reason: null,
    };
  }
  return { main: { key: "open-backup", text: "Abrir mi copia", active: true }, secondary: null, state: "Descargado", reason: null };
}

export function localModel(pack: PackUi, platform: Platform): DetailModel {
  const origin = pack.origin;
  return {
    origin: "local",
    pack: pack.id,
    name: pack.name,
    author: origin.kind === "community" ? origin.original_author : pack.author,
    icon: pack.icon,
    count: pack.stickers.length,
    animated: pack.kind === null ? null : pack.kind === "animated",
    license: origin.kind === "community" ? origin.license : null,
    category: pack.category,
    favorite: pack.favorite,
    stickers: pack.stickers.map(({ id, path, emojis, favorite, media_origin: mediaOrigin }) =>
      mediaOrigin === undefined ? { id, path, emojis, favorite } : { id, path, emojis, favorite, mediaOrigin },
    ),
    checks: pack.checks,
    bar: localBar(pack, platform),
    withdrawn: withdrawnReason(pack),
    version: origin.kind === "community" ? `Versión ${origin.version}` : null,
    communityId: origin.kind === "community" ? origin.id : null,
  };
}

export function communityModel(
  detail: CommunityDetailUi,
  library: LibraryUi | null,
  downloading: boolean,
): DetailModel {
  const { pack } = detail;
  const backup = library?.packs.find((local) => local.id === pack.downloaded) ?? null;
  return {
    origin: "community",
    pack: backup?.id ?? null,
    name: pack.name,
    author: pack.author,
    icon: pack.icon,
    count: pack.stickers,
    animated: pack.animated,
    license: pack.license,
    category: null,
    favorite: backup?.favorite ?? null,
    stickers: detail.stickers.map(({ path, emojis }) => ({ id: null, path, emojis, favorite: false })),
    checks: [],
    bar: communityBar(pack, backup, downloading),
    withdrawn: backup === null ? null : withdrawnReason(backup),
    version: pack.updatable ? `Versión ${pack.version}: hay una actualización` : `Versión ${pack.version}`,
    communityId: pack.id,
  };
}

export function checkText(check: CheckUi): string {
  return `${check.ok ? "Correcto" : "Falta"}: ${check.text}`;
}

export function moveOne(ids: readonly number[], index: number, delta: -1 | 1): number[] | null {
  const destination = index + delta;
  if (index < 0 || index >= ids.length || destination < 0 || destination >= ids.length) return null;
  return moveTo(ids, index, destination);
}

export function moveTo(ids: readonly number[], from: number, until: number): number[] | null {
  if (from === until || from < 0 || until < 0 || from >= ids.length || until >= ids.length) return null;
  const result = [...ids];
  const [moved] = result.splice(from, 1);
  if (moved === undefined) return null;
  result.splice(until, 0, moved);
  return result;
}

const COMMUNITY_LINK = "https://github.com/Duk3lo/conversor-stickers-community";

export function withdrawnUrl(packId: string): string {
  return `${COMMUNITY_LINK}/issues/new?template=retirada.yml&title=${encodeURIComponent(`Retirada: ${packId}`)}`;
}

export const CONTRIBUTE_URL = `${COMMUNITY_LINK}/blob/main/CONTRIBUTING.md`;
