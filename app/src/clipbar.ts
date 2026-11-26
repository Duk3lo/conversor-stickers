import { clipRangeDuration, outputDuration, secondsText, shortSecondsText, type ClipRange } from "./crop.ts";
import { MAX_SOUND_S } from "./sound.ts";

export const NUDGE_S = 0.1;
export const RESTORED_NOTE = "Se ha vuelto al tramo original";

/** Videos longer than this also get the zoomed bar, which never spans less than it. */
const ZOOM_SPAN_S = 30;
const SNAP_MARGIN_S = 0.3;
const SNAP_MARGIN_MAX_S = 1;
const SNAP_MARGIN_PX = 4;

/** The seconds of the video that a clip bar covers from its left edge to its right one. */
export interface BarSpan {
  readonly from: number;
  readonly to: number;
}

function tenths(seconds: number): number {
  return Math.round(seconds * 10);
}

function range(startTenths: number, endTenths: number): ClipRange {
  return { start: startTenths / 10, end: endTenths / 10 };
}

function videoLimit(durationS: number): number {
  return Math.floor(durationS * 10 + 1e-6);
}

/** Moves the whole range so it starts at `startS`, keeping its length inside the video. */
export function moveRange(current: ClipRange, startS: number, durationS: number): ClipRange {
  const length = tenths(current.end) - tenths(current.start);
  const start = Math.min(Math.max(tenths(startS), 0), Math.max(videoLimit(durationS) - length, 0));
  return range(start, start + length);
}

/** Makes the range as long as `maxRangeS` allows, keeping the start unless the video ends first. */
export function fitToTime(current: ClipRange, maxRangeS: number, durationS: number): ClipRange {
  const limit = videoLimit(durationS);
  const length = Math.min(Math.max(Math.floor(maxRangeS * 10 + 1e-6), 1), limit);
  const start = Math.min(tenths(current.start), limit - length);
  return range(Math.max(start, 0), Math.max(start, 0) + length);
}

/** Longest input range that fits: the duration cap at this speed and, when known, the size estimate. */
export function maxRangeS(
  preset: { max_duration_s: number } | null,
  speed: number,
  estimatedS: number | null,
): number | null {
  if (preset === null) return null;
  const cap = preset.max_duration_s * speed;
  return estimatedS === null ? cap : Math.min(cap, estimatedS);
}

/**
 * The longest range, in seconds of the original video and truncated to tenths, that fits everything:
 * the video, the destination at this speed, the size estimate when known and the sticker sound.
 */
export function recommendedDurationS(input: {
  durationS: number;
  preset: { max_duration_s: number } | null;
  speed: number;
  estimatedS: number | null;
  withSound: boolean;
}): number | null {
  const longest = maxRangeS(input.preset, input.speed, input.estimatedS);
  if (longest === null) return null;
  const shortest = Math.min(longest, input.durationS, input.withSound ? MAX_SOUND_S : Infinity);
  return Math.max(videoLimit(shortest), 1) / 10;
}

export function editorRecommendedS(editor: {
  duration: number | null;
  preset: { max_duration_s: number } | null;
  speed: number;
  estimate: { kind: string; estimate?: { max_range_s: number } };
  destination: string;
  sound: { kind: string };
}): number | null {
  if (editor.duration === null) return null;
  return recommendedDurationS({
    durationS: editor.duration,
    preset: editor.preset,
    speed: editor.speed,
    estimatedS: readyMaxRange(editor.estimate),
    // The sound is available exactly when the video can have one to save.
    withSound: editor.destination === "sticker" && editor.sound.kind !== "unavailable",
  });
}

export function durationFits(seconds: number, durationS: number, barLimitS: number): boolean {
  return tenths(seconds) <= videoLimit(durationS) && tenths(seconds) <= videoLimit(barLimitS);
}

/** The window of the zoomed bar around the range, with room to drag it, or null for short videos. */
export function zoomSpan(current: ClipRange, durationS: number): BarSpan | null {
  if (durationS <= ZOOM_SPAN_S) return null;
  const length = Math.min(Math.max(ZOOM_SPAN_S, 2 * (current.end - current.start)), durationS);
  const centred = (current.start + current.end - length) / 2;
  const from = Math.min(Math.max(centred, 0), durationS - length);
  return { from, to: from + length };
}

export function barFraction(second: number, span: BarSpan): number {
  return Math.min(Math.max((second - span.from) / (span.to - span.from), 0), 1);
}

export function secondAt(fraction: number, span: BarSpan): number {
  return span.from + Math.min(Math.max(fraction, 0), 1) * (span.to - span.from);
}

/**
 * How close to the original range a released drag has to be to go back to it. The general bar of a
 * long video is coarse, so there a few pixels count too; the zoomed bar passes null.
 */
export function snapMarginS(generalBar: { durationS: number; widthPx: number } | null): number {
  if (generalBar === null || generalBar.widthPx <= 0) return SNAP_MARGIN_S;
  const pixels = (SNAP_MARGIN_PX * generalBar.durationS) / generalBar.widthPx;
  return Math.min(Math.max(SNAP_MARGIN_S, pixels), SNAP_MARGIN_MAX_S);
}

/** The original range, when a drag from elsewhere leaves both ends close to it. */
export function snapToOriginal(
  original: ClipRange,
  before: ClipRange,
  released: ClipRange,
  marginS: number,
): ClipRange | null {
  const close = (a: number, b: number): boolean => Math.abs(a - b) <= marginS + 1e-9;
  const atOriginal = (range: ClipRange): boolean => close(range.start, original.start) && close(range.end, original.end);
  // A drag that starts that close is a fine adjustment, which snapping back would undo.
  return !atOriginal(before) && atOriginal(released) ? original : null;
}

/** The part of the video, as fractions of its length, that the range covers beyond what fits. */
export function shadedRegion(
  current: ClipRange,
  maxRange: number,
  durationS: number,
): { left: number; width: number } | null {
  const from = tenths(current.start) + tenths(maxRange);
  const to = tenths(current.end);
  if (to <= from || durationS <= 0) return null;
  const total = durationS * 10;
  return { left: from / total, width: (to - from) / total };
}

export function clipBarLabels(
  current: ClipRange,
  speed: number,
): { start: string; end: string; length: string; output: string } {
  return {
    start: secondsText(current.start),
    end: secondsText(current.end),
    length: secondsText(clipRangeDuration(current)),
    output: shortSecondsText(outputDuration(clipRangeDuration(current), speed)),
  };
}

export function readyMaxRange(estimate: { kind: string; estimate?: { max_range_s: number } }): number | null {
  return estimate.kind === "ready" && estimate.estimate !== undefined ? estimate.estimate.max_range_s : null;
}
