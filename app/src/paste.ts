import type { MediaUi } from "./contract.ts";
import { sizeText } from "./queue.ts";
import type { Destination, Section } from "./navigation.ts";

export const PASTED_LIMIT_BYTES = 50_000_000;

const PASTEABLE_KINDS: readonly string[] = [
  "image/png",
  "image/jpeg",
  "image/webp",
  "image/gif",
  "video/mp4",
  "video/webm",
  "video/quicktime",
];

export interface PastedFile {
  readonly type: string;
  readonly size: number;
}

export interface Clipboard<A extends PastedFile = PastedFile> {
  readonly items: ArrayLike<{ readonly kind: string; readonly type: string; getAsFile(): A | null }>;
  readonly types: ArrayLike<string>;
  getData(kind: string): string;
}

export type Pasteable<A extends PastedFile = PastedFile> =
  | { readonly kind: "file"; readonly file: A; readonly mime: string }
  | { readonly kind: "too-much-large"; readonly bytes: number }
  | { readonly kind: "kind-not-allowed"; readonly mime: string }
  | { readonly kind: "files-of-team" }
  | { readonly kind: "links"; readonly text: string }
  | { readonly kind: "nothing" };

const GNOME_KIND = "x-special/gnome-copied-files";

export function pasteableKind(kinds: readonly string[]): string | null {
  return kinds.find((kind) => PASTEABLE_KINDS.includes(kind)) ?? null;
}

export function linksOfText(text: string): string[] {
  const links = text.match(/\bhttps?:\/\/\S+/gi) ?? [];
  return [...new Set(links)];
}

export function classifyPasted<A extends PastedFile>(data: Clipboard<A>): Pasteable<A> {
  const files: A[] = [];
  for (const element of Array.from(data.items)) {
    if (element.kind !== "file") continue;
    const file = element.getAsFile();
    if (file !== null) files.push(file);
  }
  const kinds = Array.from(data.types);
  const valid = files.find((file) => PASTEABLE_KINDS.includes(file.type));
  if (valid !== undefined) {
    if (valid.size > PASTED_LIMIT_BYTES) return { kind: "too-much-large", bytes: valid.size };
    if (valid.size === 0) return { kind: "nothing" };
    return { kind: "file", file: valid, mime: valid.type };
  }
  const paths = data.getData("text/uri-list");
  if (kinds.includes(GNOME_KIND) || /^file:/im.test(paths)) return { kind: "files-of-team" };
  const first = files[0];
  if (first !== undefined) return { kind: "kind-not-allowed", mime: first.type || "unknown" };
  const links = linksOfText(`${data.getData("text/plain")}\n${paths}`);
  if (links.length > 0) return { kind: "links", text: links.join("\n") };
  return { kind: "nothing" };
}

export function pastedMessage(
  pasteable: Extract<Pasteable, { kind: "too-much-large" | "kind-not-allowed" | "files-of-team" }>,
): string {
  switch (pasteable.kind) {
    case "too-much-large":
      return `Lo pegado pesa ${sizeText(pasteable.bytes)} y el máximo es ${sizeText(PASTED_LIMIT_BYTES)}. Guárdalo y elígelo con Imagen o Video.`;
    case "kind-not-allowed":
      return `No se puede pegar un archivo ${pasteable.mime}. Vale una imagen PNG, JPEG, WebP o GIF, o un video MP4, WebM o MOV.`;
    case "files-of-team":
      return "Para archivos del equipo usa Imagen o Video.";
  }
}

export type PastedDestination =
  | { readonly kind: "editor"; readonly destination: Destination }
  | { readonly kind: "ready"; readonly notice: string | null };

export function destinationOfPasted(section: Section, media: MediaUi | null): PastedDestination {
  if (media === null) {
    return section === "download"
      ? { kind: "ready", notice: null }
      : { kind: "ready", notice: "Lo pegado no se pudo abrir: el motivo está en Descargar." };
  }
  switch (section) {
    case "stickers":
      return { kind: "editor", destination: "sticker" };
    case "gifs":
      return media.kind === "video"
        ? { kind: "editor", destination: "gif" }
        : { kind: "ready", notice: "Una imagen fija no sirve para un GIF: la tienes en Descargar para hacer un sticker." };
    case "download":
    case "search-gif":
      return { kind: "ready", notice: null };
  }
}
