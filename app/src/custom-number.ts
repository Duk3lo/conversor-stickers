export type CustomNumber = { kind: "empty" } | { kind: "ok"; value: number } | { kind: "error"; message: string };

interface Range {
  readonly min: number;
  readonly max: number;
}

function digits(text: string): number | null {
  const clean = text.trim();
  return /^\d{1,5}$/.test(clean) ? Number(clean) : null;
}

export function validateFps(text: string, range: Range): CustomNumber {
  if (text.trim() === "") return { kind: "empty" };
  const value = digits(text);
  if (value === null) return { kind: "error", message: "Escribe un número entero de fps." };
  if (value < range.min || value > range.max) {
    return { kind: "error", message: `Los fps deben estar entre ${range.min} y ${range.max}.` };
  }
  return { kind: "ok", value };
}

/** The side is rounded to the nearest even number (video encoders need it). */
export function validateSide(text: string, range: Range): CustomNumber {
  if (text.trim() === "") return { kind: "empty" };
  const value = digits(text);
  if (value === null) return { kind: "error", message: "Escribe un número entero de píxeles." };
  const even = Math.round(value / 2) * 2;
  if (even < range.min || even > range.max) {
    return { kind: "error", message: `El tamaño debe estar entre ${range.min} y ${range.max} px.` };
  }
  return { kind: "ok", value: even };
}
