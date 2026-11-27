import { asErrorUi } from "./contract.ts";

interface StillFollowerOptions {
  readonly delayMs: number;
  readonly schedule: (run: () => void, delayMs: number) => unknown;
  readonly cancel: (handle: unknown) => void;
  readonly ask: (id: number, startS: number) => Promise<string>;
  readonly apply: (path: string) => void;
  readonly fail: (failure: unknown) => void;
}

export interface StillFollower {
  /** Asks for the frame at `startS` once the start stops changing; the same start is asked once. */
  follow(id: number, startS: number): void;
  /** A new editor session: pending and in-flight requests no longer count. */
  reset(): void;
}

/** A cancelled request is the app's own doing (the entry changed or closed): nothing to tell. */
export function shouldReportStillFailure(failure: unknown): boolean {
  return asErrorUi(failure).kind !== "cancelled";
}

export function createStillFollower(options: StillFollowerOptions): StillFollower {
  let askedS: number | null = null;
  let timer: unknown;
  let turn = 0;
  let session = 0;

  return {
    follow(id, startS) {
      if (startS === askedS) return;
      askedS = startS;
      if (timer !== undefined) options.cancel(timer);
      const sessionAtRequest = session;
      const myTurn = ++turn;
      const current = () => sessionAtRequest === session && myTurn === turn;
      timer = options.schedule(() => {
        options
          .ask(id, startS)
          .then((path) => {
            if (current()) options.apply(path);
          })
          .catch((failure: unknown) => {
            if (!current()) return;
            askedS = null;
            if (shouldReportStillFailure(failure)) options.fail(failure);
          });
      }, options.delayMs);
    },
    reset() {
      if (timer !== undefined) options.cancel(timer);
      timer = undefined;
      askedS = null;
      session += 1;
    },
  };
}
