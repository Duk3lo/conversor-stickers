import type { EstimateUi } from "./contract.ts";
import {
  clipRangeDuration,
  fitMax,
  shortSecondsText,
  speedText,
  type ClipRange,
} from "./crop.ts";

const MIN_FLOOR_S = 0.1;

export type FitPlan =
  | { readonly kind: "unchanged" }
  | { readonly kind: "faster"; readonly speed: number; readonly clipRange: ClipRange }
  | { readonly kind: "shorter"; readonly speed: number; readonly clipRange: ClipRange }
  | { readonly kind: "impossible" };

// `max_seconds_s` counts seconds of the source video that fit at the current priority and speed,
// so the output duration that fits is `max_seconds_s / speed`; the same speed/shortening
// arithmetic as «Ajustar al máximo» applies to that.
export function planFit(input: { estimate: EstimateUi; clipRange: ClipRange; speed: number }): FitPlan {
  const { estimate, clipRange, speed } = input;
  if (estimate.fits) return { kind: "unchanged" };
  if (estimate.max_seconds_s < MIN_FLOOR_S) return { kind: "impossible" };
  const seconds = clipRangeDuration(clipRange);
  const fittingOutputS = estimate.max_seconds_s / speed;
  if (seconds / speed <= fittingOutputS + 1e-6) return { kind: "unchanged" };
  const fitting = fitMax(seconds, fittingOutputS);
  if (fitting.cropS === null) return { kind: "faster", speed: Math.max(fitting.speed, speed), clipRange };
  const end = Math.max(Math.round((clipRange.end - fitting.cropS) * 10) / 10, clipRange.start + MIN_FLOOR_S);
  return { kind: "shorter", speed: Math.max(fitting.speed, speed), clipRange: { start: clipRange.start, end } };
}

export function fitPlanText(plan: FitPlan): string | null {
  switch (plan.kind) {
    case "unchanged":
      return null;
    case "faster":
      return `Velocidad subida a ${speedText(plan.speed)} para que quepa.`;
    case "shorter":
      return `Tramo acortado a ${shortSecondsText(clipRangeDuration(plan.clipRange))} y velocidad ${speedText(plan.speed)} para que quepa.`;
    case "impossible":
      return "Ni así cabe: prueba con un tramo mucho más corto o con otro destino.";
  }
}

export function detailText(estimate: EstimateUi): string | null {
  return estimate.detail_px < 512 ? `Detalle reducido a ${estimate.detail_px} px` : null;
}
