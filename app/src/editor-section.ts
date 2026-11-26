import { Channel } from "@tauri-apps/api/core";

import type { BotMonitor } from "./bot-monitor.ts";
import { closeBotPanelDialog, showBotPanelDialog, watchVisibility } from "./bot-panel.ts";
import { ANDROID_DELIVERED, desktopDelivered, runBotSend } from "./bot-send.ts";
import { acceptedBot } from "./bot-status.ts";
import type { Context } from "./context.ts";
import * as commands from "./contract.ts";
import { type LibraryUi, asErrorUi, type PresetUi, type ProgressUi, type SaveVideoEvent } from "./contract.ts";
import * as mobileCommands from "./contract-mobile.ts";
import { sentToWhatsappNotice } from "./share.ts";
import { createEstimator, durationFix, reasonForCreateNo, requestOfEditor, fittingText } from "./editor.ts";
import { easyReset, easyStep, easySummary, loadMode, RESET_NOTE, saveMode, type EditorMode } from "./easy.ts";
import { planFit, fitPlanText } from "./fit.ts";
import { element } from "./dom.ts";
import { extensionCaption, loadExtensions } from "./extensions.ts";
import { localStore } from "./store.ts";
import { fitPriority, fpsAllowed } from "./fps.ts";
import { DEFAULT_FRAMING } from "./framing.ts";
import { errorMessage } from "./messages.ts";
import { previewMode } from "./mobile.ts";
import { closeEditor, type Destination, type Navigation } from "./navigation.ts";
import { compatiblePacks } from "./packs.ts";
import { createPreview, fromForClipRange, type Window } from "./preview.ts";
import {
  type ClipRange,
  boundSpeed,
  fitMax,
  fitClipRange,
  barScope,
  clipRangeDuration,
  moveEnd,
  moveStart,
  clipRangeByDefault,
} from "./crop.ts";
import { RESTORED_NOTE, editorRecommendedS, fitToTime, moveRange, snapToOriginal } from "./clipbar.ts";
import { platform } from "./platform.ts";
import { MAX_SOUND_S, initialSound, soundRequest } from "./sound.ts";
import { createEditorView } from "./editor-view.ts";
import type { Creation, EditorView } from "./views.ts";

const IMAGE_KINDS: Record<string, string> = { webp: "image/webp", gif: "image/gif", png: "image/png" };
const PRESET_STICKER = "whatsapp-sticker";
const THUMBNAILS = 10;

export interface ReadyGif {
  entry: number;
  name: string;
  destination: string;
  sizeBytes: number;
  bytes: ArrayBuffer;
  kind: string;
}

export interface EditorSection {
  onNavigate(nav: Navigation): void;
  creating(): boolean;
}

export function createEditorSection(
  context: Context,
  dependencies: {
    fileUrl(path: string): string;
    library(): LibraryUi | null;
    setLibrary(library: LibraryUi): void;
    onGifCreate(gif: ReadyGif): void;
    /** `null` in `play`, which has no bot. */
    bot: BotMonitor | null;
  },
): EditorSection {
  let view: EditorView | null = null;
  let all: PresetUi[] = [];
  let session = 0;
  let window: Window | null = null;
  // The range the app put when the video was opened, which a drag can snap back to.
  let originalClipRange: ClipRange | null = null;
  let estimateKey: string | null = null;
  const store = localStore();
  let mode: EditorMode = loadMode(store);
  // Set while «Crear sticker» of the easy mode is fitting and waiting to create.
  let easy: { fitted: boolean; before: { speed: number; clipRange: ClipRange } } | null = null;
  // Changes made by the easy flow itself must not cancel it.
  let internal = false;

  const byFile = previewMode(platform) === "file";

  const preview = createPreview({
    async request(id, fromS) {
      if (byFile) return dependencies.fileUrl(await mobileCommands.previewPath(id));
      const bytes = await commands.preview(id, fromS);
      return URL.createObjectURL(new Blob([bytes], { type: "video/webm" }));
    },
    cancel() {
      if (byFile) return;
      commands.cancelPreview().catch((failure: unknown) => {
        console.error("no se pudo cancelar la vista previa", asErrorUi(failure).message);
      });
    },
    free: (url) => {
      if (!byFile) URL.revokeObjectURL(url);
    },
    onChange(preview) {
      if (view !== null) update({ preview });
    },
  });

  const estimator = createEstimator({
    estimate: commands.estimate,
    onChange(estimate) {
      if (view !== null) update({ estimate });
      applyPendingFit();
      driveEasy();
    },
  });

  function presetsDe(destination: Destination): PresetUi[] {
    return destination === "gif" ? all.filter((preset) => preset.name.startsWith("discord")) : [];
  }

  function presetFor(destination: Destination, name: string | null): PresetUi | null {
    if (destination === "sticker") return all.find((preset) => preset.name === PRESET_STICKER) ?? null;
    const list = presetsDe(destination);
    return list.find((preset) => preset.name === name) ?? list[0] ?? null;
  }

  function packsOf(editor: EditorView): EditorView["packs"] {
    const library = dependencies.library();
    if (editor.destination !== "sticker" || library === null) return { open: null, compatible: [] };
    const compatible = compatiblePacks(library, editor.entry.media.kind === "video" ? "animated" : "static");
    const open = compatible.find((pack) => pack.id === context.nav().openedPack) ?? null;
    return { open, compatible };
  }

  function dropImage(creation: Creation): void {
    if (creation.kind === "done" && creation.image !== null) URL.revokeObjectURL(creation.image);
  }

  function update(changes: Partial<EditorView>): void {
    if (view === null) return;
    const previous = view.creation;
    view = { ...view, ...changes };
    view = { ...view, packs: packsOf(view) };
    if (changes.creation !== undefined) {
      const follows = view.creation.kind === "done" && previous.kind === "done" && view.creation.image === previous.image;
      if (!follows) dropImage(previous);
    }
    screen.render(view);
    followClipRange();
    estimate();
  }

  function followClipRange(): void {
    if (view === null || view.entry.media.kind !== "video" || view.clipRange === null || view.duration === null) {
      window = null;
    } else {
      const current = window?.id === view.entry.id ? window.fromS : null;
      window = { id: view.entry.id, fromS: byFile ? 0 : fromForClipRange(view.clipRange, view.duration, current) };
    }
    preview.show(window);
  }

  function estimate(): void {
    if (view === null || view.creation.kind === "creating") return;
    const { preset, clipRange } = view;
    const requestable = view.entry.media.kind === "video" && preset !== null && clipRange !== null && reasonForCreateNo(view) === null;
    const request = requestable ? requestOfEditor({
          id: view.entry.id,
          preset,
          clipRange,
          speed: view.speed,
          priority: view.priority,
          fps: view.fps,
          framing: view.framing,
          sidePx: view.sidePx,
        }) : null;
    const key = JSON.stringify(request);
    if (key === estimateKey) return;
    estimateKey = key;
    estimator.request(request);
  }

  function applyPendingFit(): void {
    if (view === null || !view.fitPending || creating()) return;
    const state = view.estimate;
    if (state.kind === "calculating" || state.kind === "without-request") return;
    if (state.kind === "failed" || view.clipRange === null) return void update({ fitPending: false });
    const plan = planFit({ estimate: state.estimate, clipRange: view.clipRange, speed: view.speed });
    const text = fitPlanText(plan);
    if (plan.kind === "faster" || plan.kind === "shorter") {
      const keep = internal;
      internal = true;
      change({ speed: plan.speed, clipRange: plan.clipRange, fitPending: false, fitting: text });
      internal = keep;
    } else {
      update({ fitPending: false, fitting: text });
    }
  }

  function creating(): boolean {
    return view?.creation.kind === "creating";
  }

  function change(changes: Partial<EditorView>): void {
    if (view === null || creating()) return;
    if (!internal) easy = null;
    // The result of an earlier send says nothing about the new settings.
    const botSend = view.botSend.kind === "done" ? ({ kind: "idle" } as const) : view.botSend;
    update({ fitting: null, clipNote: null, easyNote: null, preparing: easy !== null, botSend, ...changes, creation: { kind: "inactive" } });
  }

  // Runs the fit of «Hacer que quepa» (once) and creates as soon as the estimate allows it.
  function driveEasy(): void {
    if (easy === null || view === null || creating()) return;
    const { clipRange, estimate: state } = view;
    if (clipRange === null) return;
    const plan = state.kind === "ready" ? planFit({ estimate: state.estimate, clipRange, speed: view.speed }) : null;
    const step = easyStep({
      state,
      reasonNo: reasonForCreateNo(view),
      fitted: easy.fitted,
      fitPending: view.fitPending,
      impossible: plan?.kind === "impossible",
    });
    switch (step.kind) {
      case "wait":
        return;
      case "fit":
        easy.fitted = true;
        return startFit();
      case "stop": {
        const text = plan === null ? "No se pudo ajustar el tramo para que quepa." : fitPlanText(plan);
        easy = null;
        return update({ preparing: false, easyNote: text ?? "No se pudo ajustar el tramo para que quepa." });
      }
      case "create": {
        const note = easySummary(easy.before, { speed: view.speed, clipRange }, easy.fitted);
        easy = null;
        update({ preparing: false, easyNote: note });
        void create();
      }
    }
  }

  function startEasy(): void {
    if (view === null || creating() || easy !== null) return;
    if (view.entry.media.kind !== "video") return void create();
    if (view.clipRange === null || view.preset === null) return;
    easy = { fitted: false, before: { speed: view.speed, clipRange: view.clipRange } };
    update({ preparing: true, easyNote: null });
    driveEasy();
  }

  function startFit(): void {
    if (view === null || creating()) return;
    const keep = internal;
    internal = true;
    const fix = durationFix(view);
    if (fix !== null) change({ speed: fix.speed, clipRange: fix.clipRange });
    change({ priority: fitPriority(view.fps), fitPending: true });
    internal = keep;
    if (view?.estimate.kind === "without-request") update({ fitPending: false });
    applyPendingFit();
    driveEasy();
  }

  function pickMode(next: EditorMode): void {
    if (view === null || creating() || next === mode) return;
    mode = next;
    saveMode(store, mode);
    if (mode === "advanced") return change({ mode });
    const reset = easyReset(view);
    change({ ...reset.settings, mode, easyNote: reset.changed ? RESET_NOTE : null });
  }

  function open(id: number, destination: Destination): void {
    const entry = context.entry(id);
    session += 1;
    easy = null;
    if (entry === undefined || entry.media === null) {
      context.notify("No se pudo abrir lo elegido en el editor.");
      context.navigate(closeEditor(context.nav()));
      return;
    }
    const media = entry.media;
    const preset = media.kind === "video" ? presetFor(destination, null) : null;
    const duration = media.kind === "video" ? media.duration_s : null;
    estimateKey = null;
    window = null;
    originalClipRange = preset && duration !== null ? clipRangeByDefault(duration, preset.max_duration_s) : null;
    view = {
      entry: { id: entry.id, name: entry.name, media },
      destination,
      presets: presetsDe(destination),
      preset,
      preview: { kind: "empty" },
      image: media.kind === "image" ? dependencies.fileUrl(media.path) : null,
      duration,
      clipRange: originalClipRange,
      speed: 1,
      fitting: null,
      clipNote: null,
      priority: "balance",
      fps: null,
      framing: DEFAULT_FRAMING,
      sidePx: null,
      thumbnails: [],
      fitPending: false,
      mode,
      preparing: false,
      easyNote: null,
      estimate: { kind: "without-request" },
      creation: { kind: "inactive" },
      sound: initialSound(media, entry.origin),
      clipVideo: { kind: "idle" },
      extensions: loadExtensions(store),
      bot: dependencies.bot?.state() ?? null,
      botSend: { kind: "idle" },
      packs: { open: null, compatible: [] },
    };
    update({});
    if (media.kind === "video") void loadThumbnails(entry.id, session);
  }

  async function loadThumbnails(id: number, sessionMy: number): Promise<void> {
    try {
      const paths = await commands.timelineThumbnails(id, THUMBNAILS);
      if (sessionMy === session && view !== null) update({ thumbnails: paths.map((path) => dependencies.fileUrl(path)) });
    } catch (failure) {
      if (sessionMy === session) {
        context.notify(`No se pudieron cargar las miniaturas del video: ${errorMessage(asErrorUi(failure))}`);
      }
    }
  }

  function close(): void {
    if (view === null) return;
    if (creating()) {
      commands.cancelCreation().catch((failure: unknown) => {
        console.error("no se pudo cancelar", asErrorUi(failure).message);
      });
    }
    session += 1;
    easy = null;
    dropImage(view.creation);
    view = null;
    window = null;
    estimateKey = null;
    preview.show(null);
    estimator.request(null);
    screen.render(null);
  }

  function setDuration(seconds: number): void {
    if (view?.clipRange == null || view.duration === null) return;
    change({ clipRange: fitToTime(view.clipRange, seconds, view.duration) });
  }

  function clipRangeLimit(): number | null {
    return view?.preset ? barScope(view.preset.max_duration_s) : null;
  }

  async function create(): Promise<void> {
    if (view === null || creating()) return;
    const { preset, clipRange, entry, destination } = view;
    const sessionMy = session;
    const esVideo = entry.media.kind === "video";
    if (esVideo && (preset === null || clipRange === null || reasonForCreateNo(view) !== null)) return;

    const channel = new Channel<ProgressUi>();
    channel.onmessage = (progress) => {
      if (sessionMy === session && view?.creation.kind === "creating") {
        update({ creation: { ...view.creation, progress } });
      }
    };
    update({ creation: { kind: "creating", progress: null, cancelling: false } });
    try {
      let result;
      let kind = "image/webp";
      if (esVideo && preset !== null && clipRange !== null) {
        result = await commands.create(
          requestOfEditor({
            id: entry.id,
            preset,
            clipRange,
            speed: view.speed,
            priority: view.priority,
            fps: view.fps,
            framing: view.framing,
            sidePx: view.sidePx,
          }),
          channel,
        );
        kind = IMAGE_KINDS[preset.extension] ?? "application/octet-stream";
      } else {
        result = await commands.createStatic({ id: entry.id, framing: view.framing });
      }
      let image: string | null = null;
      if (result.meets_limit) {
        const bytes = await commands.result(entry.id);
        if (sessionMy !== session) return;
        image = URL.createObjectURL(new Blob([bytes], { type: kind }));
        if (destination === "gif" && preset !== null) {
          dependencies.onGifCreate({
            entry: entry.id,
            name: entry.name,
            destination: preset.title,
            sizeBytes: result.size_bytes,
            bytes,
            kind,
          });
        }
      }
      if (sessionMy !== session) return;
      update({
        creation: {
          kind: "done",
          result,
          image,
          saving: false,
          saved: null,
          saveError: null,
          sentNotice: null,
          adding: false,
          addedTo: null,
          addError: null,
        },
      });
    } catch (failure) {
      if (sessionMy !== session) return;
      const error = asErrorUi(failure);
      update({ creation: error.kind === "cancelled" ? { kind: "inactive" } : { kind: "error", error } });
    }
  }

  async function cancelCreation(): Promise<void> {
    const current = view?.creation;
    if (current?.kind !== "creating" || current.cancelling) return;
    update({ creation: { ...current, cancelling: true } });
    try {
      await commands.cancelCreation();
    } catch (failure) {
      const now = view?.creation;
      if (now?.kind === "creating") update({ creation: { ...now, cancelling: false } });
      console.error("no se pudo cancelar", asErrorUi(failure).message);
    }
  }

  async function save(): Promise<void> {
    const current = view?.creation;
    if (view === null || current?.kind !== "done" || current.saving) return;
    const sessionMy = session;
    update({ creation: { ...current, saving: true, saveError: null } });
    try {
      const name = await commands.save(view.entry.id);
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") {
        update({ creation: { ...now, saving: false, saved: name ?? now.saved } });
      }
    } catch (failure) {
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") {
        update({ creation: { ...now, saving: false, saveError: errorMessage(asErrorUi(failure)) } });
      }
    }
  }

  async function saveSound(): Promise<void> {
    if (view === null || view.clipRange === null || view.sound.kind === "saving" || view.sound.kind === "unavailable") return;
    const { startS, seconds } = soundRequest(view.clipRange);
    const sessionMy = session;
    const id = view.entry.id;
    update({ sound: { kind: "saving" } });
    try {
      const name = await commands.saveSound(id, startS, seconds, new Channel<SaveVideoEvent>());
      if (sessionMy === session) update({ sound: name === null ? { kind: "idle" } : { kind: "saved", name } });
    } catch (failure) {
      if (sessionMy === session) update({ sound: { kind: "failed", message: errorMessage(asErrorUi(failure)) } });
    }
  }

  async function saveClipVideo(): Promise<void> {
    if (view === null || view.clipRange === null || view.clipVideo.kind === "saving") return;
    const { startS, seconds } = soundRequest(view.clipRange);
    const sessionMy = session;
    const id = view.entry.id;
    update({ clipVideo: { kind: "saving" } });
    try {
      const name = await commands.saveClipVideo(id, startS, seconds, new Channel<SaveVideoEvent>());
      if (sessionMy === session) update({ clipVideo: name === null ? { kind: "idle" } : { kind: "saved", name } });
    } catch (failure) {
      if (sessionMy === session) update({ clipVideo: { kind: "failed", message: errorMessage(asErrorUi(failure)) } });
    }
  }

  async function sendToExtension(id: string): Promise<void> {
    const target = view?.extensions.find((installed) => installed.extension.id === id);
    if (view === null || view.clipRange === null || view.clipVideo.kind !== "saved" || target?.number == null) return;
    const soundS = view.clipRange.end - view.clipRange.start;
    try {
      // The saved clip is already cut to the chosen range, so only the sound length goes in the caption.
      await mobileCommands.sendToBot(target.number, extensionCaption(target.extension, soundS, view.framing));
    } catch (failure) {
      context.notify(errorMessage(asErrorUi(failure)));
    }
  }

  async function sendClipToBot(): Promise<void> {
    const monitor = dependencies.bot;
    if (monitor === null || view === null || view.clipRange === null || view.duration === null) return;
    if (creating() || view.botSend.kind === "running" || view.clipVideo.kind === "saving") return;
    const target = acceptedBot(view.extensions);
    const number = target?.number ?? null;
    if (target === null || number === null) return;
    if (clipRangeDuration(view.clipRange) > MAX_SOUND_S) {
      change({
        clipRange: fitToTime(view.clipRange, MAX_SOUND_S, view.duration),
        clipNote: `El tramo se ha recortado a ${MAX_SOUND_S} s para el bot.`,
      });
    }
    if (view?.clipRange == null) return;
    const sessionMy = session;
    const id = view.entry.id;
    const { startS, seconds } = soundRequest(view.clipRange);
    // Rounded to the tenths of the bar: 4.999999 s must not become `sonido=4`.
    const caption = extensionCaption(target.extension, Math.round(seconds * 10) / 10, view.framing);
    let savedName: string | null = null;
    update({ botSend: { kind: "running", text: "Consultando el bot…" }, clipVideo: { kind: "idle" } });
    const outcome = await runBotSend({
      status: () => monitor.run((api) => api.status()),
      // The monitor queues the request, so the editor may be gone by the time it runs.
      activate: () => monitor.run((api) => (sessionMy === session ? api.activate() : api.status())),
      async save() {
        const name = await commands.saveClipVideo(id, startS, seconds, new Channel<SaveVideoEvent>());
        savedName = name;
        return name;
      },
      async deliver(name) {
        if (platform === "android") {
          await mobileCommands.sendToBot(number, caption);
          return ANDROID_DELIVERED;
        }
        // The webview may refuse to copy after the save dialog: then «Copiar pie» does it.
        const copied = caption !== null && (await navigator.clipboard.writeText(caption).then(() => true, () => false));
        await monitor.openChat(number);
        return desktopDelivered(name, caption, copied);
      },
      abandoned: () => sessionMy !== session,
      progress(text) {
        if (sessionMy === session) update({ botSend: { kind: "running", text } });
      },
    });
    if (sessionMy !== session) return context.notify(outcome.message);
    const saved = savedName as string | null;
    update({
      botSend: { kind: "done", ...outcome, caption: platform === "desktop" && saved !== null ? caption : null },
      // A saved clip that was not sent stays available to the manual send.
      ...(saved !== null && !outcome.sent ? { clipVideo: { kind: "saved", name: saved } as const } : {}),
    });
  }

  function openBotPanel(): void {
    if (!creating()) showBotPanelDialog();
  }

  async function share(): Promise<void> {
    const current = view?.creation;
    if (view === null || current?.kind !== "done" || current.saving) return;
    const sessionMy = session;
    update({ creation: { ...current, saving: true, saveError: null } });
    try {
      await mobileCommands.shareResult(view.entry.id);
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") update({ creation: { ...now, saving: false } });
    } catch (failure) {
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") {
        update({ creation: { ...now, saving: false, saveError: errorMessage(asErrorUi(failure)) } });
      }
    }
  }

  async function sendToWhatsapp(): Promise<void> {
    const current = view?.creation;
    if (view === null || current?.kind !== "done" || current.saving) return;
    const sessionMy = session;
    update({ creation: { ...current, saving: true, saveError: null, sentNotice: null } });
    try {
      const { result, notice } = await mobileCommands.sendResultToWhatsapp(view.entry.id);
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") {
        update({ creation: { ...now, saving: false, sentNotice: sentToWhatsappNotice(result, notice) } });
      }
    } catch (failure) {
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") {
        update({ creation: { ...now, saving: false, saveError: errorMessage(asErrorUi(failure)) } });
      }
    }
    await refreshLibrary();
  }

  /** The quick pack is created or changed in Rust, also when WhatsApp then refuses or is cancelled. */
  async function refreshLibrary(): Promise<void> {
    try {
      dependencies.setLibrary(await commands.library());
    } catch (failure) {
      console.error("no se pudo recargar la biblioteca", failure);
    }
  }

  async function addPack(id: number): Promise<void> {
    const current = view?.creation;
    if (view === null || current?.kind !== "done" || current.adding) return;
    const sessionMy = session;
    update({ creation: { ...current, adding: true, addError: null } });
    try {
      const library = await commands.addSticker(id, view.entry.id);
      dependencies.setLibrary(library);
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") {
        const name = library.packs.find((pack) => pack.id === id)?.name ?? null;
        update({ creation: { ...now, adding: false, addedTo: name } });
      }
    } catch (failure) {
      const now = view?.creation;
      if (sessionMy === session && now?.kind === "done") {
        update({ creation: { ...now, adding: false, addError: errorMessage(asErrorUi(failure)) } });
      }
    }
  }

  const screen = createEditorView({
    back() {
      if (!creating()) context.navigate(closeEditor(context.nav()));
    },
    pickPreset(name) {
      if (view === null) return;
      const preset = presetFor(view.destination, name);
      const fps = fpsAllowed(preset, view.fps) ? view.fps : null;
      if (preset === null || view.duration === null || view.clipRange === null) return change({ preset, fps, sidePx: null });
      change({ preset, fps, sidePx: null, clipRange: fitClipRange(view.clipRange, view.duration, barScope(preset.max_duration_s)) });
    },
    moveStart(seconds) {
      const limit = clipRangeLimit();
      if (view?.clipRange == null || view.duration === null || limit === null) return;
      change({ clipRange: moveStart(view.clipRange, seconds, view.duration, limit) });
    },
    moveEnd(seconds) {
      const limit = clipRangeLimit();
      if (view?.clipRange == null || view.duration === null || limit === null) return;
      change({ clipRange: moveEnd(view.clipRange, seconds, view.duration, limit) });
    },
    moveRange(startS) {
      if (view?.clipRange == null || view.duration === null) return;
      change({ clipRange: moveRange(view.clipRange, startS, view.duration) });
    },
    pickDuration: setDuration,
    autoFit() {
      if (view === null) return;
      const recommended = editorRecommendedS(view);
      if (recommended !== null) setDuration(recommended);
    },
    releaseDrag(before, marginS) {
      if (view?.clipRange == null || originalClipRange === null) return;
      const original = snapToOriginal(originalClipRange, before, view.clipRange, marginS);
      if (original !== null) change({ clipRange: original, clipNote: RESTORED_NOTE });
    },
    pickSide: (sidePx) => change({ sidePx }),
    changeSpeed: (speed) => change({ speed: boundSpeed(speed) }),
    fitMax() {
      if (view?.clipRange == null || view.preset === null) return;
      const fitting = fitMax(clipRangeDuration(view.clipRange), view.preset.max_duration_s);
      change({ speed: fitting.speed });
      if (fitting.cropS !== null) update({ fitting: fittingText(fitting.cropS) });
    },
    pickPriority: (priority) => change({ priority }),
    pickFps: (fps) => change({ fps }),
    setFraming: (framing) => change({ framing }),
    makeFit: startFit,
    pickMode,
    create: () => (view?.mode === "easy" ? startEasy() : void create()),
    cancelCreation: () => void cancelCreation(),
    save: () => void save(),
    saveSound: () => void saveSound(),
    saveClipVideo: () => void saveClipVideo(),
    openFolder: (folder) => context.openFolder(folder),
    openWhatsapp: () => context.openWhatsapp(),
    sendToExtension: (id) => void sendToExtension(id),
    openBotPanel,
    sendToBot: () => void sendClipToBot(),
    share: () => void share(),
    sendToWhatsapp: () => void sendToWhatsapp(),
    addPack: (id) => void addPack(id),
  });
  screen.render(null);

  const bot = dependencies.bot;
  if (bot !== null) {
    watchVisibility(bot, "editor", element("bot-box"));
    bot.subscribe((state) => {
      if (view === null) return;
      view = { ...view, bot: state };
      screen.render(view);
    });
  }

  commands
    .presets()
    .then((presets) => {
      all = presets;
      if (view === null) return;
      const preset = view.entry.media.kind === "video" ? presetFor(view.destination, null) : null;
      let clipRange = view.clipRange;
      if (preset && view.duration !== null && clipRange === null) {
        clipRange = originalClipRange = clipRangeByDefault(view.duration, preset.max_duration_s);
      }
      update({ presets: presetsDe(view.destination), preset, clipRange });
    })
    .catch((failure: unknown) => {
      context.notify(`No se pudieron cargar los destinos: ${errorMessage(asErrorUi(failure))}`);
    });

  return {
    onNavigate(nav) {
      const target = nav.editor;
      if (target === null) {
        // «Atrás» on the phone can leave the editor while the dialog is open over it.
        closeBotPanelDialog();
        return close();
      }
      if (view?.entry.id === target.entry && view.destination === target.destination) return;
      open(target.entry, target.destination);
    },
    creating,
  };
}
