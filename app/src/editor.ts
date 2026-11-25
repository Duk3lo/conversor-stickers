import { sizeText } from "./queue.ts";
import {
  asErrorUi,
  type EstimateUi,
  type CreateRequest,
  type FramingSpecUi,
  type PresetUi,
  type PriorityUi,
} from "./contract.ts";
import {
  outputDuration,
  clipRangeDuration,
  type ClipRange,
  secondsText,
  shortSecondsText,
  speedText,
  fitsClipRange,
  fitMax,
  MAX_FITTING_SPEED,
} from "./crop.ts";
import type { EstimateState } from "./views.ts";

export const ESTIMATE_WAITS_MS = 400;

export const Priorities: readonly PriorityUi[] = ["sharpness", "balance", "smoothness", "fit_anyway", "fluid"];

export function priorityText(priority: PriorityUi): { title: string; explanation: string } {
  switch (priority) {
    case "sharpness":
      return {
        title: "Más calidad",
        explanation: "Imagen nítida. Si no cabe, te dice cuántos segundos caben.",
      };
    case "balance":
      return { title: "Equilibrado", explanation: "El punto medio entre nitidez, fluidez y duración." };
    case "smoothness":
      return {
        title: "Más duración",
        explanation: "Más fluido y más segundos, bajando algo la nitidez.",
      };
    case "fit_anyway":
      return {
        title: "Que quepa",
        explanation: "Baja fluidez, nitidez y detalle todo lo que haga falta para que quepa. Se nota más.",
      };
    case "fluid":
      return {
        title: "Más fluido",
        explanation: "Mantiene los fps elegidos; baja nitidez y colores para que quepa.",
      };
  }
}

export function estimateText(estimate: EstimateUi): string {
  const size = `≈ ${sizeText(estimate.estimated_size_bytes)} de ${sizeText(estimate.max_size_bytes)}`;
  return `${size} · ${estimate.fits ? "cabe" : "no cabe"} · ${estimate.fps} fps`;
}

export function maxSecondsText(estimate: EstimateUi): string {
  if (estimate.max_seconds_s < 0.1) {
    return "Con esta calidad no cabe ni una décima del video: prueba con «Más duración»";
  }
  return `Con esta calidad caben unos ${shortSecondsText(estimate.max_seconds_s)} del video`;
}

export function outputDurationText(clipRangeS: number, speed: number): string {
  return `${secondsText(clipRangeS)} del video a ${speedText(speed)} · dura ${secondsText(outputDuration(clipRangeS, speed))}`;
}

export function fittingText(cropS: number): string {
  return `No entra ni a ${speedText(MAX_FITTING_SPEED)}: acorta el tramo ${shortSecondsText(cropS)} o sube la velocidad a mano.`;
}

export function reasonForCreateNo(editor: {
  preset: PresetUi | null;
  clipRange: ClipRange | null;
  speed: number;
}): string | null {
  const { preset, clipRange, speed } = editor;
  if (preset === null) return "Cargando los destinos…";
  if (clipRange === null) return "Elige el tramo del video.";
  const seconds = clipRangeDuration(clipRange);
  if (!fitsClipRange(seconds, preset.max_duration_s, speed)) {
    return (
      `A ${speedText(speed)}, el tramo de ${shortSecondsText(seconds)} dura ` +
      `${shortSecondsText(outputDuration(seconds, speed))} y el máximo es ` +
      `${shortSecondsText(preset.max_duration_s)}. Sube la velocidad o pulsa «Ajustar al máximo».`
    );
  }
  return null;
}

export function durationFix(editor: {
  preset: PresetUi | null;
  clipRange: ClipRange | null;
  speed: number;
}): { speed: number; clipRange: ClipRange } | null {
  const { preset, clipRange, speed } = editor;
  if (preset === null || clipRange === null) return null;
  const seconds = clipRangeDuration(clipRange);
  if (fitsClipRange(seconds, preset.max_duration_s, speed)) return null;
  const fitting = fitMax(seconds, preset.max_duration_s);
  const end = fitting.cropS === null ? clipRange.end : Math.round((clipRange.end - fitting.cropS) * 10) / 10;
  return { speed: Math.max(fitting.speed, speed), clipRange: { start: clipRange.start, end } };
}

export function requestOfEditor(editor: {
  id: number;
  preset: PresetUi;
  clipRange: ClipRange;
  speed: number;
  priority: PriorityUi;
  fps: number | null;
  framing: FramingSpecUi;
  sidePx: number | null;
}): CreateRequest {
  return {
    id: editor.id,
    preset: editor.preset.name,
    start_s: editor.clipRange.start,
    seconds: clipRangeDuration(editor.clipRange),
    speed: editor.speed,
    priority: editor.priority,
    fps: editor.fps,
    framing: editor.framing,
    side_px: editor.sidePx,
  };
}

export function createEstimator(dependencies: {
  estimate(request: CreateRequest): Promise<EstimateUi>;
  onChange(state: EstimateState): void;
  wait?(ms: number): Promise<void>;
}): { request(request: CreateRequest | null): void } {
  const wait = dependencies.wait ?? ((ms: number) => new Promise<void>((follow) => setTimeout(follow, ms)));
  let last = 0;

  async function estimate(number: number, request: CreateRequest): Promise<void> {
    await wait(ESTIMATE_WAITS_MS);
    if (number !== last) return;
    let state: EstimateState;
    try {
      state = { kind: "ready", estimate: await dependencies.estimate(request) };
    } catch (failure) {
      const error = asErrorUi(failure);
      state = error.kind === "cancelled" ? { kind: "without-request" } : { kind: "failed", message: error.message };
    }
    if (number === last) dependencies.onChange(state);
  }

  return {
    request(request) {
      last += 1;
      if (request === null) {
        dependencies.onChange({ kind: "without-request" });
        return;
      }
      dependencies.onChange({ kind: "calculating" });
      void estimate(last, request);
    },
  };
}
