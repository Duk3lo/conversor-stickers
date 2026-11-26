import type { FramingSpecUi } from "./contract.ts";

export const MIN_ZOOM = 1;
export const MAX_ZOOM = 4;
export const ZOOM_STEP = 0.05;

export const DEFAULT_FRAMING: FramingSpecUi = {
  mode: "crop",
  center_x: 0.5,
  center_y: 0.5,
  zoom: 1,
  background: { kind: "transparent" },
  shape: "square",
  scale: 1,
  align: "center",
};

export interface CropSquare {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

function limit(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function halves(zoom: number, width: number, height: number): { x: number; y: number } {
  const side = Math.min(width, height) / zoom;
  return { x: side / width / 2, y: side / height / 2 };
}

export function clampFraming(spec: FramingSpecUi, width: number, height: number): FramingSpecUi {
  const zoom = Number.isFinite(spec.zoom) ? limit(spec.zoom, MIN_ZOOM, MAX_ZOOM) : MIN_ZOOM;
  const half = halves(zoom, width, height);
  const x = Number.isFinite(spec.center_x) ? spec.center_x : 0.5;
  const y = Number.isFinite(spec.center_y) ? spec.center_y : 0.5;
  return {
    ...spec,
    center_x: limit(x, half.x, 1 - half.x),
    center_y: limit(y, half.y, 1 - half.y),
    zoom,
  };
}

export function cropSquare(spec: FramingSpecUi, width: number, height: number): CropSquare {
  const clamped = clampFraming(spec, width, height);
  const half = halves(clamped.zoom, width, height);
  return {
    left: clamped.center_x - half.x,
    top: clamped.center_y - half.y,
    width: half.x * 2,
    height: half.y * 2,
  };
}

export function dragFraming(
  start: FramingSpecUi,
  dxPx: number,
  dyPx: number,
  shownWidthPx: number,
  shownHeightPx: number,
  width: number,
  height: number,
): FramingSpecUi {
  if (shownWidthPx <= 0 || shownHeightPx <= 0) return clampFraming(start, width, height);
  return clampFraming(
    { ...start, center_x: start.center_x + dxPx / shownWidthPx, center_y: start.center_y + dyPx / shownHeightPx },
    width,
    height,
  );
}

export function withZoom(spec: FramingSpecUi, zoom: number, width: number, height: number): FramingSpecUi {
  return clampFraming({ ...spec, zoom }, width, height);
}

/** Two fingers: the zoom grows as much as the distance between them did. */
export function pinchFraming(
  start: FramingSpecUi,
  startDistancePx: number,
  distancePx: number,
  width: number,
  height: number,
): FramingSpecUi {
  const ratio = startDistancePx > 0 ? distancePx / startDistancePx : 1;
  return withZoom(start, start.zoom * ratio, width, height);
}

export function framingResetText(spec: FramingSpecUi): string | null {
  const moved = spec.center_x !== 0.5 || spec.center_y !== 0.5 || spec.zoom !== 1;
  return moved ? "Centrar y quitar el zoom" : null;
}

export function zoomText(zoom: number): string {
  return `${String(Math.round(zoom * 100) / 100).replace(".", ",")}x`;
}
