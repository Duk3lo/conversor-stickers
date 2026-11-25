import { asErrorUi, PREVIEW_WINDOW_S } from "./contract.ts";
import type { ClipRange } from "./crop.ts";

const WAITS_MS = 200;

export type PreviewState =
  | { readonly kind: "empty" }
  | { readonly kind: "loading"; readonly fromS: number }
  | { readonly kind: "ready"; readonly url: string; readonly fromS: number }
  | { readonly kind: "failed"; readonly fromS: number; readonly message: string };

export interface Window {
  readonly id: number;
  readonly fromS: number;
}

export function fromForClipRange(clipRange: ClipRange, durationS: number, currentS: number | null): number {
  if (durationS <= PREVIEW_WINDOW_S) return 0;
  if (currentS !== null && clipRange.start >= currentS && clipRange.end <= currentS + PREVIEW_WINDOW_S) {
    return currentS;
  }
  const centered = Math.floor((clipRange.start + clipRange.end - PREVIEW_WINDOW_S) / 2);
  return Math.min(Math.max(centered, 0), durationS - PREVIEW_WINDOW_S);
}

export function secondInPreview(secondS: number, fromS: number): number {
  return Math.max(secondS - fromS, 0);
}

export function createPreview(dependencies: {
  request(id: number, fromS: number): Promise<string>;
  cancel(): void;
  free(url: string): void;
  onChange(state: PreviewState): void;
  wait?(ms: number): Promise<void>;
}): { show(window: Window | null): void } {
  const wait = dependencies.wait ?? ((ms: number) => new Promise<void>((follow) => setTimeout(follow, ms)));
  let desired: Window | null = null;
  let url: string | null = null;
  let inGear = false;
  let requesting = false;

  async function generate(): Promise<void> {
    inGear = true;
    while (desired !== null) {
      const window: Window = desired;
      await wait(WAITS_MS);
      if (window !== desired) continue;
      let state: PreviewState;
      requesting = true;
      try {
        const fresh = await dependencies.request(window.id, window.fromS);
        if (window !== desired) {
          dependencies.free(fresh);
          continue;
        }
        url = fresh;
        state = { kind: "ready", url: fresh, fromS: window.fromS };
      } catch (failure) {
        if (window !== desired) continue;
        state = { kind: "failed", fromS: window.fromS, message: asErrorUi(failure).message };
      } finally {
        requesting = false;
      }
      inGear = false;
      dependencies.onChange(state);
      return;
    }
    inGear = false;
  }

  return {
    show(window) {
      if (window?.id === desired?.id && window?.fromS === desired?.fromS) return;
      desired = window;
      const previous = url;
      url = null;
      dependencies.onChange(
        window === null ? { kind: "empty" } : { kind: "loading", fromS: window.fromS },
      );
      if (previous !== null) dependencies.free(previous);
      if (requesting) dependencies.cancel();
      if (!inGear && window !== null) void generate();
    },
  };
}
