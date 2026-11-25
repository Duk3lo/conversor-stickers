import { clipRangeDuration, outputDuration, secondsText, shortSecondsText, type ClipRange } from "./crop.ts";

export const NUDGE_S = 0.1;

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
