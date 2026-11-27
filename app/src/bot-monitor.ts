// The one place that talks to the bot: it keeps its last state, asks again only while somebody can
// see it, and never has two requests in flight.
import type { BotApi, BotStatusUi } from "./contract-bot.ts";
import { asErrorUi } from "./contract.ts";
import { type BotState, INITIAL_BOT_STATE, botEnabled, pollDelayMs } from "./bot-status.ts";

export interface Timers {
  set(run: () => void, delayMs: number): number;
  clear(timer: number): void;
}

const windowTimers: Timers = {
  set: (run, delayMs) => window.setTimeout(run, delayMs),
  clear: (timer) => window.clearTimeout(timer),
};

export interface BotMonitor {
  state(): BotState;
  subscribe(listener: (state: BotState) => void): void;
  load(): Promise<void>;
  connect(url: string, token: string | null): Promise<void>;
  connectLink(link: string): Promise<void>;
  /** Desktop: connects to the bot of this computer if nothing is saved; the state stays as it was otherwise. */
  connectLocal(): Promise<void>;
  /** Saves the link whose address the person just confirmed. */
  acceptLink(): Promise<void>;
  /** The app stops asking; the bot is not touched. */
  disable(): Promise<void>;
  /** Runs an operation that answers the new status; a failure is thrown after reading the status again. */
  run(operation: (api: BotApi) => Promise<BotStatusUi>): Promise<BotStatusUi>;
  openChat(number: string): Promise<void>;
  /** Says whether the view `key` is on screen. */
  watch(key: string, visible: boolean): void;
  /** `false` while the whole app is in the background. */
  setForeground(foreground: boolean): void;
}

export function createBotMonitor(api: BotApi, timers: Timers = windowTimers): BotMonitor {
  let state = INITIAL_BOT_STATE;
  const listeners: ((state: BotState) => void)[] = [];
  const watchers = new Set<string>();
  let foreground = true;
  let timer: number | null = null;
  let queue: Promise<unknown> = Promise.resolve();
  let pending = 0;
  // Changes with the connection, so an answer that belongs to the previous one is dropped.
  let generation = 0;

  function set(next: BotState): void {
    if (JSON.stringify(next) === JSON.stringify(state)) return;
    state = next;
    for (const listener of listeners) listener(state);
  }

  function watched(): boolean {
    return foreground && watchers.size > 0 && botEnabled(state);
  }

  function stopTimer(): void {
    if (timer !== null) timers.clear(timer);
    timer = null;
  }

  function schedule(): void {
    stopTimer();
    const delay = pollDelayMs(state);
    if (pending > 0 || !watched() || delay === null) return;
    timer = timers.set(() => {
      timer = null;
      void poll();
    }, delay);
  }

  /** Requests wait for each other; polling resumes when none is left. */
  function inTurn<T>(request: () => Promise<T>): Promise<T> {
    pending += 1;
    stopTimer();
    const result = queue.then(request).finally(() => {
      pending -= 1;
      schedule();
    });
    // The queue only keeps the order: each caller handles its own failure.
    queue = result.catch(() => undefined);
    return result;
  }

  async function readStatus(): Promise<void> {
    if (!botEnabled(state)) return;
    const asked = generation;
    try {
      const status = await api.status();
      if (asked === generation) set({ ...state, status, error: null });
    } catch (failure) {
      if (asked === generation) set({ ...state, status: null, error: asErrorUi(failure) });
    }
  }

  function poll(): Promise<void> {
    return inTurn(readStatus);
  }

  function resume(): void {
    if (!watched()) return stopTimer();
    if (pending === 0 && pollDelayMs(state) !== null) void poll();
  }

  async function change(next: () => Promise<NonNullable<BotState["connection"]>>): Promise<void> {
    const connection = await next();
    generation += 1;
    stopTimer();
    set({ connection, status: null, error: null });
    resume();
  }

  return {
    state: () => state,
    subscribe: (listener) => void listeners.push(listener),
    async load() {
      try {
        await change(() => api.connection());
      } catch (failure) {
        set({ connection: null, status: null, error: asErrorUi(failure) });
      }
    },
    connect: (url, token) => change(() => api.connect(url, token)),
    connectLink: (link) => change(() => api.connectLink(link)),
    async connectLocal() {
      const connection = await api.connectLocal();
      if (connection !== null) await change(async () => connection);
    },
    acceptLink: () => change(() => api.acceptLink()),
    disable: () => change(() => api.disable()),
    run(operation) {
      const asked = generation;
      return inTurn(async () => {
        if (asked !== generation || !botEnabled(state)) throw new Error("la conexión con el bot ha cambiado");
        try {
          const status = await operation(api);
          if (asked === generation) set({ ...state, status, error: null });
          return status;
        } catch (failure) {
          await readStatus();
          throw failure;
        }
      });
    },
    openChat: (number) => api.openChat(number),
    watch(key, visible) {
      const before = watched();
      if (visible) watchers.add(key);
      else watchers.delete(key);
      if (before !== watched()) resume();
    },
    setForeground(next) {
      const before = watched();
      foreground = next;
      if (before !== watched()) resume();
    },
  };
}
