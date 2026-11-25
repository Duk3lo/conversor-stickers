export interface ClipRange {
  readonly start: number;
  readonly end: number;
}

export const STEP_S = 0.1;
export const LARGE_STEP_S = 1;

const DURATION_BY_DEFAULT_S = 6;

function tenths(seconds: number): number {
  return Math.round(seconds * 10);
}

function seconds(tenthsOfSecond: number): number {
  return tenthsOfSecond / 10;
}

function endLimit(durationS: number): number {
  return Math.floor(durationS * 10 + 1e-6);
}

function clipRange(start: number, end: number): ClipRange {
  return { start: seconds(start), end: seconds(end) };
}

function wholeVideo(durationS: number): ClipRange {
  return { start: 0, end: durationS };
}

export function clipRangeByDefault(durationS: number, capS: number): ClipRange {
  const limit = endLimit(durationS);
  if (limit < 1) return wholeVideo(durationS);
  return clipRange(0, Math.min(tenths(DURATION_BY_DEFAULT_S), tenths(capS), limit));
}

export function fitClipRange(current: ClipRange, durationS: number, maxS: number): ClipRange {
  const limit = endLimit(durationS);
  if (limit < 1) return wholeVideo(durationS);
  const start = Math.min(Math.max(tenths(current.start), 0), limit - 1);
  const end = Math.min(Math.max(tenths(current.end), start + 1), limit, start + tenths(maxS));
  return clipRange(start, end);
}

export function moveStart(current: ClipRange, valueS: number, durationS: number, maxS: number): ClipRange {
  const limit = endLimit(durationS);
  if (limit < 1) return wholeVideo(durationS);
  let end = Math.min(tenths(current.end), limit);
  const start = Math.min(Math.max(tenths(valueS), 0), end - 1);
  end = Math.min(end, start + tenths(maxS));
  return clipRange(start, end);
}

export function moveEnd(current: ClipRange, valueS: number, durationS: number, maxS: number): ClipRange {
  const limit = endLimit(durationS);
  if (limit < 1) return wholeVideo(durationS);
  let start = Math.min(Math.max(tenths(current.start), 0), limit - 1);
  const end = Math.min(Math.max(tenths(valueS), start + 1), limit);
  start = Math.max(start, end - tenths(maxS));
  return clipRange(start, end);
}

export function clipRangeDuration(current: ClipRange): number {
  return seconds(tenths(current.end) - tenths(current.start));
}

export function secondsText(valueS: number): string {
  return `${valueS.toFixed(1).replace(".", ",")} s`;
}

export function clipRangeText(current: ClipRange): string {
  return `${secondsText(current.start)} → ${secondsText(current.end)} · ${secondsText(clipRangeDuration(current))}`;
}

export const MIN_SPEED = 0.5;
export const MAX_SPEED = 3;
export const MAX_FITTING_SPEED = 2;
export const SPEED_STEP = 0.05;

const TOLERANCE_S = 1e-6;

export function maxOfClipRange(capS: number, speed: number): number {
  return capS * speed;
}

export function barScope(capS: number): number {
  return maxOfClipRange(capS, MAX_SPEED);
}

export function fitsClipRange(clipRangeS: number, capS: number, speed: number): boolean {
  return clipRangeS / speed <= capS + TOLERANCE_S;
}

export function outputDuration(clipRangeS: number, speed: number): number {
  return clipRangeS / speed;
}

export function boundSpeed(value: number): number {
  if (!Number.isFinite(value)) return 1;
  const hundredths = Math.round(value / SPEED_STEP) * 5;
  return Math.min(Math.max(hundredths / 100, MIN_SPEED), MAX_SPEED);
}

export function fitMax(clipRangeS: number, capS: number): { speed: number; cropS: number | null } {
  const steps = Math.ceil((clipRangeS / capS) / SPEED_STEP - 1e-9);
  const speed = Math.round(steps * 5) / 100;
  if (speed <= 1) return { speed: 1, cropS: null };
  if (speed <= MAX_FITTING_SPEED) return { speed, cropS: null };
  const extra = clipRangeS - maxOfClipRange(capS, MAX_FITTING_SPEED);
  return { speed: MAX_FITTING_SPEED, cropS: Math.ceil(extra * 10 - 1e-6) / 10 };
}

export function speedText(speed: number): string {
  return `${String(Math.round(speed * 100) / 100).replace(".", ",")}x`;
}

export function shortSecondsText(valueS: number): string {
  return `${String(Math.round(valueS * 10) / 10).replace(".", ",")} s`;
}
