import { sizeText } from "./queue.ts";
import type { ProgressUi, CreateResult } from "./contract.ts";
import { secondsText } from "./crop.ts";

export function progressText(progress: ProgressUi | null): string {
  if (progress === null) return "Preparando…";
  switch (progress.kind) {
    case "decoding":
      return "Leyendo el video…";
    case "preview":
      return "Preparando la vista previa…";
    case "fitting_size":
      return `Ajustando el peso · intento ${progress.attempt} de ${progress.max}`;
    case "searching_duration":
      return `No cabe; buscando una duración que quepa · ${progress.attempt} de ${progress.max}`;
  }
}

export function progressFraction(progress: ProgressUi | null): number | null {
  if (progress === null) return null;
  if (progress.kind === "fitting_size" || progress.kind === "searching_duration") {
    return progress.max > 0 ? Math.min(1, progress.attempt / progress.max) : null;
  }
  return null;
}

export function resultSizeText(result: CreateResult): string {
  return `${sizeText(result.size_bytes)} de ${sizeText(result.max_size_bytes)}`;
}

export function resultDetail(result: CreateResult): string {
  const reductions =
    result.reductions.length === 0 ? "Sin reducciones" : `Reducido: ${result.reductions.join(", ")}`;
  const parts = [reductions];
  if (result.duration_s !== null) parts.unshift(secondsText(result.duration_s));
  if (result.fps !== null) parts.unshift(`${result.fps} fps`);
  return parts.join(" · ");
}
