import type { PresetUi, PriorityUi } from "./contract.ts";

const FPS_CHOICES: readonly number[] = [15, 20, 24, 30];

/** The fps chips for a destination: automatic (null) plus the choices it allows. */
export function fpsChoices(preset: PresetUi | null): readonly (number | null)[] {
  const allowed = preset === null ? [] : FPS_CHOICES.filter((fps) => fps <= preset.fps_range.max);
  return [null, ...allowed];
}

export function fpsAllowed(preset: PresetUi | null, fps: number | null): boolean {
  if (fps === null) return true;
  return preset !== null && Number.isInteger(fps) && fps >= preset.fps_range.min && fps <= preset.fps_range.max;
}

export function fpsLabel(fps: number | null): string {
  return fps === null ? "Auto" : String(fps);
}

/** «Que quepa» lowers fps, so with a chosen fps the fit has to go through «Más fluido» instead. */
export function fitPriority(fps: number | null): PriorityUi {
  return fps === null ? "fit_anyway" : "fluid";
}
