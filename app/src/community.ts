import type { PackUi } from "./contract.ts";
import type { CommunityProgressUi } from "./contract-community.ts";
import { foreignOriginReason } from "./klipy.ts";

export const COMMUNITY_NOTICE =
  "Los paquetes de la comunidad los suben otras personas. Se revisan antes de publicarse, pero puede haber errores. " +
  "Si ves algo tuyo que no debería estar, puedes solicitar su retirada desde la página del paquete.";

export interface AllowedLicense {
  readonly id: string;
  readonly name: string;
}

export const Licenses: readonly AllowedLicense[] = [
  { id: "CC0-1.0", name: "CC0 (dominio público)" },
  { id: "CC-BY-4.0", name: "CC BY 4.0 (con atribución)" },
  { id: "CC-BY-SA-4.0", name: "CC BY-SA 4.0 (atribución y compartir igual)" },
];

const PROPOSAL_STICKERS_MIN = 3;
const PROPOSAL_STICKERS_MAX = 30;

export function downloadPercentage(progress: CommunityProgressUi | null): number | null {
  if (progress === null || progress.kind !== "downloading" || progress.total <= 0) return null;
  return Math.min(100, Math.round((progress.done / progress.total) * 100));
}

export function proposableNoReason(pack: PackUi): string | null {
  if (pack.origin.kind === "community") return "Este paquete ya es de la comunidad: propón uno propio.";
  const foreign = foreignOriginReason(pack);
  if (foreign !== null) return foreign;
  const count = pack.stickers.length;
  if (count < PROPOSAL_STICKERS_MIN || count > PROPOSAL_STICKERS_MAX) {
    return `Un paquete de la comunidad lleva de ${PROPOSAL_STICKERS_MIN} a ${PROPOSAL_STICKERS_MAX} stickers y este tiene ${count}.`;
  }
  if (pack.icon === null) return "Falta el icono del paquete.";
  if (pack.author.trim() === "") return "Pon un autor al paquete: aparecerá en el catálogo.";
  if (new Set(pack.stickers.map((sticker) => sticker.kind)).size > 1) return "Un paquete no mezcla stickers estáticos y animados.";
  return null;
}

export type ProposalValidation =
  | { readonly valid: true; readonly license: string; readonly category: string }
  | { readonly valid: false; readonly reason: string };

export function validateProposal(
  license: string,
  category: string,
  categories: readonly { readonly id: string }[],
): ProposalValidation {
  if (!Licenses.some((allowed) => allowed.id === license)) return { valid: false, reason: "Elige una de las tres licencias." };
  if (!categories.some((candidate) => candidate.id === category)) return { valid: false, reason: "Elige una categoría del catálogo." };
  return { valid: true, license, category };
}

export interface CommunityErrorText {
  readonly community: string;
  readonly start: string;
  readonly detail: string | null;
}

export function communityErrorText(technicalMessage: string): CommunityErrorText {
  const detail = technicalMessage.trim();
  return {
    community: "La comunidad aún no está disponible o no hay conexión.",
    start: "La comunidad no está disponible ahora.",
    detail: detail === "" ? null : detail,
  };
}
