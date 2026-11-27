import type { FramingSpecUi, PriorityUi } from "./contract.ts";
import { clipRangeDuration, shortSecondsText, speedText, type ClipRange } from "./crop.ts";
import { DEFAULT_FRAMING } from "./framing.ts";
import type { Store } from "./store.ts";
import type { EstimateState } from "./views.ts";

export type EditorMode = "easy" | "advanced";

const MODE_KEY = "editor-mode";

export const RESET_NOTE = "Los ajustes avanzados volvieron a los valores del modo Fácil.";

export interface EasySettings {
  readonly speed: number;
  readonly priority: PriorityUi;
  readonly fps: number | null;
  readonly framing: FramingSpecUi;
  readonly sidePx: number | null;
}

export const EASY_DEFAULTS: EasySettings = {
  speed: 1,
  priority: "balance",
  fps: null,
  framing: DEFAULT_FRAMING,
  sidePx: null,
};

export function loadMode(store: Store | null): EditorMode {
  try {
    return store?.getItem(MODE_KEY) === "advanced" ? "advanced" : "easy";
  } catch {
    return "easy";
  }
}

export function saveMode(store: Store | null, mode: EditorMode): void {
  try {
    store?.setItem(MODE_KEY, mode);
  } catch (failure) {
    console.error("no se pudo recordar el modo del editor", failure);
  }
}

export function easyReset(current: EasySettings): { settings: EasySettings; changed: boolean } {
  // How the video is framed is also chosen in easy mode, so it stays.
  const { mode, center_x, center_y, zoom } = current.framing;
  const settings = { ...EASY_DEFAULTS, framing: { ...DEFAULT_FRAMING, mode, center_x, center_y, zoom } };
  const changed = (Object.keys(settings) as (keyof EasySettings)[]).some(
    (key) => JSON.stringify(current[key]) !== JSON.stringify(settings[key]),
  );
  return { settings, changed };
}

export type EasyStep = { readonly kind: "wait" | "fit" | "create" | "stop" };

// Decides what «Crear sticker» does next. The fit runs at most once (`fitted`); after it, the
// result is created even if the estimate is still marginal, because the real result reports it.
export function easyStep(input: {
  state: EstimateState;
  reasonNo: string | null;
  fitted: boolean;
  fitPending: boolean;
  impossible: boolean;
}): EasyStep {
  if (input.fitPending) return { kind: "wait" };
  if (input.reasonNo !== null) return { kind: input.fitted ? "stop" : "fit" };
  switch (input.state.kind) {
    case "calculating":
    case "without-request":
      return { kind: "wait" };
    case "failed":
      return { kind: "create" };
    case "ready":
      if (input.state.estimate.fits) return { kind: "create" };
      if (!input.fitted) return { kind: "fit" };
      return { kind: input.impossible ? "stop" : "create" };
  }
}

export function easySummary(
  before: { speed: number; clipRange: ClipRange },
  after: { speed: number; clipRange: ClipRange },
  lowered: boolean,
): string | null {
  const parts: string[] = [];
  if (clipRangeDuration(after.clipRange) < clipRangeDuration(before.clipRange) - 1e-6) {
    parts.push(`se acortó el tramo a ${shortSecondsText(clipRangeDuration(after.clipRange))}`);
  }
  if (after.speed > before.speed + 1e-6) parts.push(`se aceleró a ${speedText(after.speed)}`);
  if (lowered) parts.push("se bajó la calidad");
  if (parts.length === 0) return null;
  const text = parts.length === 1 ? parts[0]! : `${parts.slice(0, -1).join(", ")} y ${parts.at(-1)!}`;
  return `${text.charAt(0).toUpperCase()}${text.slice(1)} para que quepa.`;
}
