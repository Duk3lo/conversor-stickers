import type { AlignUi, BackgroundUi, FramingSpecUi, ShapeUi, SideRangeUi } from "./contract.ts";

export const SCALES: readonly number[] = [1, 0.85, 0.7, 0.5];
const MIN_SCALE = 0.5;

export const PALETTE: readonly { rgb: string; name: string }[] = [
  { rgb: "#ffffff", name: "Blanco" },
  { rgb: "#000000", name: "Negro" },
  { rgb: "#ef4444", name: "Rojo" },
  { rgb: "#f59e0b", name: "Naranja" },
  { rgb: "#22c55e", name: "Verde" },
  { rgb: "#3b82f6", name: "Azul" },
  { rgb: "#8b5cf6", name: "Violeta" },
  { rgb: "#ec4899", name: "Rosa" },
];

const DEFAULT_COLOUR = "#ffffff";

export function scaleText(scale: number): string {
  return `${Math.round(scale * 100)} %`;
}

export function isHexColour(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

export function withBackground(spec: FramingSpecUi, kind: BackgroundUi["kind"]): FramingSpecUi {
  if (kind === "color") {
    const rgb = spec.background.kind === "color" ? spec.background.rgb : DEFAULT_COLOUR;
    return { ...spec, background: { kind, rgb } };
  }
  return { ...spec, background: { kind } };
}

export function withColour(spec: FramingSpecUi, rgb: string): FramingSpecUi {
  if (!isHexColour(rgb)) return spec;
  return { ...spec, background: { kind: "color", rgb: rgb.toLowerCase() } };
}

export function withShape(spec: FramingSpecUi, shape: ShapeUi): FramingSpecUi {
  return { ...spec, shape };
}

export function withAlign(spec: FramingSpecUi, align: AlignUi): FramingSpecUi {
  return { ...spec, align };
}

export function withScale(spec: FramingSpecUi, scale: number): FramingSpecUi {
  return { ...spec, scale: Math.min(Math.max(scale, MIN_SCALE), 1) };
}

/** `null` is the preset's own side. Empty when the preset has a fixed canvas. */
export function sideChoices(preset: { side_range_px: SideRangeUi | null }): {
  value: number | null;
  label: string;
}[] {
  if (preset.side_range_px === null) return [];
  return [
    { value: null, label: "Original" },
    ...preset.side_range_px.suggested.map((side) => ({ value: side, label: `${side} px` })),
  ];
}

export function shapeRadius(shape: ShapeUi): string {
  return shape === "square" ? "0" : shape === "rounded" ? "14%" : "50%";
}

/** The square that holds the framed content (background, shape mask), as fractions of the canvas. */
export function contentBox(spec: FramingSpecUi): { left: number; top: number; size: number } {
  const size = spec.mode === "fit" ? spec.scale : 1;
  return { left: (1 - size) / 2, top: (1 - size) / 2, size };
}

/** Where the whole source is drawn inside the content square, as fractions of it (CSS approximation). */
export function fitLayout(
  spec: FramingSpecUi,
  width: number,
  height: number,
): { left: number; top: number; width: number; height: number } {
  const shownWidth = width >= height ? 1 : width / height;
  const shownHeight = width >= height ? height / width : 1;
  const free = 1 - shownHeight;
  const top = spec.mode !== "fit" || spec.align === "center" ? free / 2 : spec.align === "top" ? 0 : free;
  return { left: (1 - shownWidth) / 2, top, width: shownWidth, height: shownHeight };
}
