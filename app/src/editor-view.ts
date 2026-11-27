import { bottomAction } from "./bottom-bar.ts";
import { sizeText } from "./queue.ts";
import { PREVIEW_WINDOW_S } from "./contract.ts";
import type { AlignUi, BackgroundUi, FramingModeUi, FramingSpecUi, PresetUi, PriorityUi, ShapeUi } from "./contract.ts";
import {
  NUDGE_S,
  barFraction,
  clipBarLabels,
  dragsWholeRange,
  durationFits,
  editorRecommendedS,
  maxRangeS,
  readyMaxRange,
  secondAt,
  shadedRegion,
  snapMarginS,
  zoomSpan,
  type BarSpan,
} from "./clipbar.ts";
import { validateFps, validateSide } from "./custom-number.ts";
import {
  PALETTE,
  SCALES,
  contentBox,
  fitLayout,
  scaleText,
  shapeRadius,
  sideChoices,
  withAlign,
  withBackground,
  withColour,
  withScale,
  withShape,
} from "./framing-options.ts";
import { create, element, icon, pressedButton, showMessage } from "./dom.ts";
import {
  reasonForCreateNo,
  durationFix,
  outputDurationText,
  estimateText,
  priorityText,
  maxSecondsText,
} from "./editor.ts";
import { detailText } from "./fit.ts";
import { fpsChoices } from "./fps.ts";
import {
  clampFraming,
  cropSquare,
  dragFraming,
  pinchFraming,
  framingResetText,
  withZoom,
  zoomText,
} from "./framing.ts";
import { errorMessage } from "./messages.ts";
import { sendTargets, soundDurationNotice, stillImageNotice } from "./extensions.ts";
import { botBox, manualTargets } from "./bot-status.ts";
import { canSaveClipVideo, clipVideoText, openWhatsappVisible } from "./clip-video.ts";
import { soundText } from "./sound.ts";
import { folderButtonVisible } from "./saved-folder.ts";
import { playableNoVideoReason, previewMode } from "./mobile.ts";
import { platform } from "./platform.ts";
import { canSendToWhatsapp, canShareResult } from "./share.ts";
import { secondInPreview, secondInVideo } from "./preview.ts";
import { usesSoftwareRendering } from "./software-rendering.ts";
import { resultDetail, progressFraction, resultSizeText, progressText } from "./progress.ts";
import {
  barScope,
  clipRangeDuration,
  LARGE_STEP_S,
  STEP_S,
  type ClipRange,
  secondsText,
  shortSecondsText,
  speedText,
} from "./crop.ts";
import type { EditorActions, Creation, EditorView } from "./views.ts";

function formatLabel(extension: string): string {
  switch (extension) {
    case "webp":
      return "WebP";
    case "png":
      return "APNG";
    default:
      return extension.toUpperCase();
  }
}

function title(view: EditorView): string {
  if (view.destination === "gif") return view.preset?.title ?? "GIF para Discord";
  return view.entry.media.kind === "image" ? "Sticker estático de WhatsApp" : "Sticker animado de WhatsApp";
}

interface ClipBar {
  readonly track: HTMLDivElement;
  readonly clipRange: HTMLDivElement;
  readonly shade: HTMLDivElement;
  readonly startHandle: HTMLDivElement;
  readonly endHandle: HTMLDivElement;
  /** What the bar covers as last painted. */
  span: BarSpan | null;
}

export function createEditorView(actions: EditorActions): { render(view: EditorView | null): void } {
  const backButton = element<HTMLButtonElement>("editor-back");
  const editorTitle = element<HTMLHeadingElement>("editor-title");
  const file = element<HTMLParagraphElement>("editor-file");
  const destinations = element<HTMLDivElement>("editor-destinations");
  const preview = element<HTMLDivElement>("preview");
  const video = element<HTMLVideoElement>("video");
  const image = element<HTMLImageElement>("editor-image");
  const previewMessage = element<HTMLParagraphElement>("preview-message");
  const videoControls = element<HTMLDivElement>("controls-video");
  const crop = element<HTMLDivElement>("crop");
  const thumbs = element<HTMLDivElement>("crop-thumbs");
  const nudges = element<HTMLDivElement>("crop-nudges");
  const cropOutput = element<HTMLParagraphElement>("crop-output");
  const zoomBar = element<HTMLDivElement>("crop-zoom");
  const zoomFrom = element<HTMLSpanElement>("crop-zoom-from");
  const zoomTo = element<HTMLSpanElement>("crop-zoom-to");
  const clipNote = element<HTMLParagraphElement>("clip-note");
  const durations = element<HTMLDivElement>("crop-durations");
  const autoFitButton = element<HTMLButtonElement>("auto-fit");
  const playWholeButton = element<HTMLButtonElement>("play-whole");
  const startHereButton = element<HTMLButtonElement>("start-here");
  const playbackNote = element<HTMLParagraphElement>("playback-note");
  const playbackButtons = element<HTMLDivElement>("playback-buttons");
  // Showing a <video> blackens the whole window on software GL, so a still of the clip stands in.
  const stillOnly = usesSoftwareRendering();
  const fpsCustom = element<HTMLInputElement>("fps-custom");
  const fpsError = element<HTMLParagraphElement>("fps-error");
  const sideBox = element<HTMLFieldSetElement>("side-box");
  const sideChips = element<HTMLDivElement>("side-chips");
  const sideCustomBox = element<HTMLDivElement>("side-custom-box");
  const sideCustom = element<HTMLInputElement>("side-custom");
  const sideError = element<HTMLParagraphElement>("side-error");
  const sideNote = element<HTMLParagraphElement>("side-note");
  const fitBox = element<HTMLDivElement>("framing-fit-box");
  const colourBox = element<HTMLDivElement>("colour-box");
  const swatches = element<HTMLDivElement>("swatches");
  const colourCustom = element<HTMLInputElement>("colour-custom");
  const scaleChips = element<HTMLDivElement>("scale-chips");
  const generalBar: ClipBar = {
    track: element<HTMLDivElement>("crop-track"),
    clipRange: element<HTMLDivElement>("crop-clip-range"),
    shade: element<HTMLDivElement>("crop-shade"),
    startHandle: element<HTMLDivElement>("handle-start"),
    endHandle: element<HTMLDivElement>("handle-end"),
    span: null,
  };
  const zoomedBar: ClipBar = {
    track: element<HTMLDivElement>("crop-zoom-track"),
    clipRange: element<HTMLDivElement>("crop-zoom-clip-range"),
    shade: element<HTMLDivElement>("crop-zoom-shade"),
    startHandle: element<HTMLDivElement>("zoom-handle-start"),
    endHandle: element<HTMLDivElement>("zoom-handle-end"),
    span: null,
  };
  const cropText = element<HTMLSpanElement>("crop-text");
  const speed = element<HTMLInputElement>("speed");
  const speedValue = element<HTMLOutputElement>("speed-value");
  const speedDetail = element<HTMLParagraphElement>("speed-detail");
  const fitButton = element<HTMLButtonElement>("fit-max");
  const fittingNotice = element<HTMLParagraphElement>("fitting-notice");
  const priority = element<HTMLFieldSetElement>("priority");
  const fpsControl = element<HTMLFieldSetElement>("fps");
  const priorityExplanation = element<HTMLParagraphElement>("priority-explanation");
  const previewFrame = element<HTMLDivElement>("preview-frame");
  const previewContent = element<HTMLDivElement>("preview-content");
  const square = element<HTMLDivElement>("framing-square");
  const framingControls = element<HTMLFieldSetElement>("controls-framing");
  const framingExplanation = element<HTMLParagraphElement>("framing-explanation");
  const zoomBox = element<HTMLDivElement>("framing-zoom-box");
  const zoom = element<HTMLInputElement>("framing-zoom");
  const zoomValue = element<HTMLOutputElement>("framing-zoom-value");
  const resetFraming = element<HTMLButtonElement>("framing-reset");
  const estimateDetail = element<HTMLParagraphElement>("estimate-detail");
  const makeFitButton = element<HTMLButtonElement>("make-fit");
  const estimate = element<HTMLDivElement>("estimate");
  const estimatedSize = element<HTMLParagraphElement>("estimate-size");
  const estimatedSeconds = element<HTMLParagraphElement>("estimate-seconds");
  const createButton = element<HTMLButtonElement>("create");
  const createBar = element<HTMLSpanElement>("create-bar");
  const createText = element<HTMLSpanElement>("create-text");
  const cancelButton = element<HTMLButtonElement>("cancel-creation");
  const createWithSoundButton = element<HTMLButtonElement>("create-with-sound");
  const clipVideoState = element<HTMLParagraphElement>("clip-video-state");
  const soundBox = element<HTMLDivElement>("sound-box");
  const soundButton = element<HTMLButtonElement>("save-sound");
  const soundState = element<HTMLParagraphElement>("sound-state");
  const clipVideoFolder = element<HTMLButtonElement>("clip-video-folder");
  const openWhatsappButton = element<HTMLButtonElement>("open-whatsapp");
  const extensionNotices = element<HTMLDivElement>("extension-notices");
  const extensionSends = element<HTMLDivElement>("extension-sends");
  const botBoxNode = element<HTMLDivElement>("bot-box");
  const botLine = element<HTMLParagraphElement>("bot-line");
  const botOpenButton = element<HTMLButtonElement>("bot-open");
  const botSendButton = element<HTMLButtonElement>("bot-send");
  const botSendNote = element<HTMLParagraphElement>("bot-send-note");
  const botCaptionBox = element<HTMLDivElement>("bot-caption-box");
  const botCaption = element<HTMLElement>("bot-caption");
  const botCaptionCopy = element<HTMLButtonElement>("bot-caption-copy");
  const botCaptionCopied = element<HTMLSpanElement>("bot-caption-copied");
  const soundFolder = element<HTMLButtonElement>("sound-folder");
  const announcement = element<HTMLParagraphElement>("announcement");
  const createError = element<HTMLDivElement>("error-create");
  const createErrorText = element<HTMLParagraphElement>("error-create-text");
  const editorPanel = element<HTMLElement>("editor");
  const modeSwitch = element<HTMLFieldSetElement>("editor-mode");
  const easyNote = element<HTMLParagraphElement>("easy-note");
  const result = element<HTMLElement>("result");
  const resultImage = element<HTMLImageElement>("result-image");
  const resultTitle = element<HTMLHeadingElement>("result-title");
  const resultMeets = element<HTMLParagraphElement>("result-meets");
  const detail = element<HTMLParagraphElement>("result-detail");
  const notice = element<HTMLParagraphElement>("result-notice");
  const openAddButton = element<HTMLButtonElement>("add-open");
  const addPick = element<HTMLDivElement>("add-pick");
  const packSelector = element<HTMLSelectElement>("add-pack");
  const addButton = element<HTMLButtonElement>("add-button");
  const withoutPacks = element<HTMLParagraphElement>("add-without-packs");
  const addState = element<HTMLParagraphElement>("add-state");
  const saveButton = element<HTMLButtonElement>("save");
  const shareButton = element<HTMLButtonElement>("share");
  const sendWhatsappButton = element<HTMLButtonElement>("send-whatsapp");
  const saved = element<HTMLParagraphElement>("saved");

  swatches.replaceChildren(
    ...PALETTE.map((colour) => {
      const swatch = create("button", "swatch");
      swatch.type = "button";
      swatch.dataset.rgb = colour.rgb;
      swatch.style.backgroundColor = colour.rgb;
      swatch.setAttribute("aria-label", colour.name);
      return swatch;
    }),
  );
  scaleChips.replaceChildren(
    ...SCALES.map((scale) => {
      const chip = create("button", "chip", scaleText(scale));
      chip.type = "button";
      chip.dataset.scale = String(scale);
      return chip;
    }),
  );

  let view: EditorView | null = null;
  let currentUrl: string | null = null;
  let fromPreview = 0;
  let failedPreview = false;
  let previousClipRange: ClipRange | null = null;
  let paintedPresets: readonly PresetUi[] | null = null;
  let announcementLast = "";

  function creating(): boolean {
    return view?.creation.kind === "creating";
  }

  backButton.addEventListener("click", () => actions.back());

  destinations.addEventListener("change", (event) => {
    if (event.target instanceof HTMLInputElement) actions.pickPreset(event.target.value);
  });

  speed.addEventListener("input", () => actions.changeSpeed(Number(speed.value)));
  fitButton.addEventListener("click", () => actions.fitMax());
  modeSwitch.addEventListener("change", (event) => {
    if (event.target instanceof HTMLInputElement) actions.pickMode(event.target.value === "easy" ? "easy" : "advanced");
  });
  priority.addEventListener("change", (event) => {
    if (event.target instanceof HTMLInputElement) actions.pickPriority(event.target.value as PriorityUi);
  });
  fpsControl.addEventListener("change", (event) => {
    if (!(event.target instanceof HTMLInputElement) || event.target.type !== "radio") return;
    fpsCustom.value = "";
    showMessage(fpsError, null);
    actions.pickFps(event.target.value === "auto" ? null : Number(event.target.value));
  });
  fpsCustom.addEventListener("input", () => {
    const range = view?.preset?.fps_range;
    if (!range) return;
    const typed = validateFps(fpsCustom.value, range);
    showMessage(fpsError, typed.kind === "error" ? typed.message : null);
    fpsCustom.setAttribute("aria-invalid", String(typed.kind === "error"));
    if (typed.kind === "ok") actions.pickFps(typed.value);
    else if (typed.kind === "empty") actions.pickFps(null);
  });
  sideChips.addEventListener("click", (event) => {
    const chip = event.target instanceof Element ? event.target.closest<HTMLButtonElement>("button[data-side]") : null;
    if (!chip || creating()) return;
    sideCustom.value = "";
    showMessage(sideError, null);
    actions.pickSide(chip.dataset.side === "" ? null : Number(chip.dataset.side));
  });
  sideCustom.addEventListener("input", () => {
    const range = view?.preset?.side_range_px;
    if (!range) return;
    const typed = validateSide(sideCustom.value, range);
    showMessage(sideError, typed.kind === "error" ? typed.message : null);
    sideCustom.setAttribute("aria-invalid", String(typed.kind === "error"));
    if (typed.kind === "ok") actions.pickSide(typed.value);
    else if (typed.kind === "empty") actions.pickSide(null);
  });
  scaleChips.addEventListener("click", (event) => {
    const chip = event.target instanceof Element ? event.target.closest<HTMLButtonElement>("button[data-scale]") : null;
    if (chip) changeFraming(withScale(view!.framing, Number(chip.dataset.scale)));
  });
  swatches.addEventListener("click", (event) => {
    const swatch = event.target instanceof Element ? event.target.closest<HTMLButtonElement>("button[data-rgb]") : null;
    if (swatch && view) changeFraming(withColour(view.framing, swatch.dataset.rgb ?? ""));
  });
  colourCustom.addEventListener("input", () => {
    if (view) changeFraming(withColour(view.framing, colourCustom.value));
  });
  makeFitButton.addEventListener("click", () => actions.makeFit());

  function sourceSize(): { width: number; height: number } | null {
    const media = view?.entry.media;
    return media ? { width: media.width, height: media.height } : null;
  }

  function changeFraming(changes: Partial<FramingSpecUi>): void {
    const size = sourceSize();
    if (view === null || size === null || creating()) return;
    actions.setFraming(clampFraming({ ...view.framing, ...changes }, size.width, size.height));
  }

  framingControls.addEventListener("change", (event) => {
    const target = event.target;
    if (target instanceof HTMLInputElement && view !== null && target.type === "radio") {
      if (target.name === "background") changeFraming(withBackground(view.framing, target.value as BackgroundUi["kind"]));
      if (target.name === "shape") changeFraming(withShape(view.framing, target.value as ShapeUi));
      if (target.name === "align") changeFraming(withAlign(view.framing, target.value as AlignUi));
    }
    if (event.target instanceof HTMLInputElement && event.target.name === "framing") {
      changeFraming({ mode: event.target.value as FramingModeUi });
    }
  });
  zoom.addEventListener("input", () => {
    const size = sourceSize();
    if (view === null || size === null || creating()) return;
    actions.setFraming(withZoom(view.framing, Number(zoom.value), size.width, size.height));
  });
  resetFraming.addEventListener("click", () => changeFraming({ center_x: 0.5, center_y: 0.5, zoom: 1 }));

  let squareDrag: { x: number; y: number; start: FramingSpecUi } | null = null;
  // Fingers on the preview while cropping, as in WhatsApp: one moves the video, two pinch it. Easy mode
  // shows only the result, without the square, so there the drag is on the preview itself.
  const fingers = new Map<number, { x: number; y: number }>();
  let pinch: { distance: number; start: FramingSpecUi } | null = null;
  let resultDrag: { x: number; y: number; start: FramingSpecUi } | null = null;
  const fingerDistance = (): number => {
    const [a, b] = [...fingers.values()];
    return a === undefined || b === undefined ? 0 : Math.hypot(a.x - b.x, a.y - b.y);
  };
  preview.addEventListener("pointerdown", (event) => {
    if (view === null || creating() || view.framing.mode !== "crop") return;
    fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    resultDrag = square.hidden && fingers.size === 1 ? { x: event.clientX, y: event.clientY, start: view.framing } : null;
    if (fingers.size === 2) {
      squareDrag = null;
      pinch = { distance: fingerDistance(), start: view.framing };
    }
  });
  preview.addEventListener("pointermove", (event) => {
    const size = sourceSize();
    if (!fingers.has(event.pointerId) || size === null) return;
    fingers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pinch !== null && fingers.size === 2) {
      actions.setFraming(pinchFraming(pinch.start, pinch.distance, fingerDistance(), size.width, size.height));
    } else if (resultDrag !== null) {
      const box = previewFrame.getBoundingClientRect();
      // The video follows the finger, so the crop moves the other way.
      const [dx, dy] = [resultDrag.x - event.clientX, resultDrag.y - event.clientY];
      actions.setFraming(dragFraming(resultDrag.start, dx, dy, box.width, box.height, size.width, size.height));
    }
  });
  const liftFinger = (event: PointerEvent): void => {
    fingers.delete(event.pointerId);
    resultDrag = null;
    if (fingers.size < 2) pinch = null;
  };
  // On the window: a finger can be lifted outside the preview.
  window.addEventListener("pointerup", liftFinger);
  window.addEventListener("pointercancel", liftFinger);
  square.addEventListener("pointerdown", (event) => {
    if (view === null || creating()) return;
    event.preventDefault();
    square.focus();
    square.setPointerCapture(event.pointerId);
    squareDrag = { x: event.clientX, y: event.clientY, start: view.framing };
  });
  square.addEventListener("pointermove", (event) => {
    const size = sourceSize();
    if (squareDrag === null || size === null) return;
    const box = previewFrame.getBoundingClientRect();
    actions.setFraming(
      dragFraming(
        squareDrag.start,
        event.clientX - squareDrag.x,
        event.clientY - squareDrag.y,
        box.width,
        box.height,
        size.width,
        size.height,
      ),
    );
  });
  const dropSquare = (): void => {
    squareDrag = null;
  };
  square.addEventListener("pointerup", dropSquare);
  square.addEventListener("pointercancel", dropSquare);
  square.addEventListener("keydown", (event) => {
    if (view === null) return;
    const step = event.shiftKey ? 0.1 : 0.02;
    const moves: Record<string, Partial<FramingSpecUi>> = {
      ArrowLeft: { center_x: view.framing.center_x - step },
      ArrowRight: { center_x: view.framing.center_x + step },
      ArrowUp: { center_y: view.framing.center_y - step },
      ArrowDown: { center_y: view.framing.center_y + step },
    };
    const move = moves[event.key];
    if (move === undefined) return;
    event.preventDefault();
    changeFraming(move);
  });

  createButton.addEventListener("click", () => {
    if (createButton.getAttribute("aria-disabled") !== "true") actions.create();
  });
  cancelButton.addEventListener("click", () => actions.cancelCreation());
  saveButton.addEventListener("click", () => actions.save());
  createWithSoundButton.addEventListener("click", () => actions.saveClipVideo());
  soundButton.addEventListener("click", () => actions.saveSound());
  clipVideoFolder.addEventListener("click", () => actions.openFolder("videos"));
  openWhatsappButton.addEventListener("click", () => actions.openWhatsapp());
  extensionSends.addEventListener("click", (event) => {
    const id = pressedButton(event)?.dataset["extension"];
    if (id !== undefined) actions.sendToExtension(id);
  });
  soundFolder.addEventListener("click", () => actions.openFolder("sounds"));
  botOpenButton.addEventListener("click", () => actions.openBotPanel());
  botSendButton.addEventListener("click", () => actions.sendToBot());
  botCaptionCopy.addEventListener("click", () => {
    navigator.clipboard.writeText(botCaption.textContent ?? "").then(
      () => setText(botCaptionCopied, "Copiado."),
      () => setText(botCaptionCopied, "No se pudo copiar: selecciona el texto y cópialo a mano."),
    );
  });
  shareButton.addEventListener("click", () => actions.share());
  sendWhatsappButton.addEventListener("click", () => actions.sendToWhatsapp());
  openAddButton.addEventListener("click", () => {
    const open = view?.packs.open;
    if (open) actions.addPack(open.id);
  });
  addButton.addEventListener("click", () => {
    if (packSelector.value !== "") actions.addPack(Number(packSelector.value));
  });

  // «Reproducir todo el video»: the range stops repeating until it changes or the user goes back.
  let playingWhole = false;

  function keepInClipRange(): void {
    const clipRange = view?.clipRange;
    if (playingWhole || !clipRange || !currentUrl || failedPreview) return;
    const start = secondInPreview(clipRange.start, fromPreview);
    const end = secondInPreview(clipRange.end, fromPreview);
    if (video.currentTime >= end || video.currentTime < start - 0.25) video.currentTime = start;
  }

  function goStartOfClipRange(): void {
    if (view?.clipRange && currentUrl) video.currentTime = secondInPreview(view.clipRange.start, fromPreview);
  }

  function watchPlayback(): void {
    keepInClipRange();
    if (!video.paused) requestAnimationFrame(watchPlayback);
  }

  function play(): void {
    video.play().catch((failure: unknown) => {
      if (failure instanceof DOMException && failure.name === "AbortError") return;
      console.warn("no se pudo reproducir la vista previa", failure);
    });
  }

  video.addEventListener("play", () => requestAnimationFrame(watchPlayback));
  video.addEventListener("timeupdate", keepInClipRange);
  video.addEventListener("loadedmetadata", goStartOfClipRange);
  video.addEventListener("ended", () => {
    if (playingWhole) video.currentTime = 0;
    else goStartOfClipRange();
    play();
  });
  video.addEventListener("error", () => {
    if (!currentUrl) return;
    failedPreview = true;
    console.warn("el video no se puede previsualizar", video.error?.message ?? "");
    renderPreviewMessage();
    if (view) renderPlayback(view);
  });
  image.addEventListener("error", () => {
    if (image.getAttribute("src")) {
      preview.dataset.state = "failed";
      previewMessage.textContent = "No se puede mostrar esta imagen, pero se puede convertir igualmente.";
    }
  });

  playWholeButton.addEventListener("click", () => {
    if (view === null) return;
    playingWhole = !playingWhole;
    if (playingWhole) video.currentTime = 0;
    else goStartOfClipRange();
    play();
    renderPlayback(view);
  });
  startHereButton.addEventListener("click", () => {
    if (view === null || creating()) return;
    const instant = secondInVideo(video.currentTime, fromPreview);
    playingWhole = false;
    actions.moveRange(instant);
    renderPlayback(view);
    play();
  });

  type Handle = "start" | "end";
  // `handle` is null while the whole range is being moved, held `grab` seconds after its start.
  let drag: { bar: ClipBar; pointerId: number; handle: Handle | null; grab: number; before: ClipRange } | null = null;

  function valueIn(bar: ClipBar, clientX: number): number | null {
    if (bar.span === null) return null;
    const box = bar.track.getBoundingClientRect();
    return secondAt((clientX - box.left) / box.width, bar.span);
  }

  function move(handle: Handle, seconds: number): void {
    if (handle === "start") actions.moveStart(seconds);
    else actions.moveEnd(seconds);
  }

  /** Ends the drag and repaints, so the zoomed bar is placed around the range again. */
  function endDrag(pointerId: number, released: boolean): void {
    if (drag === null || drag.pointerId !== pointerId) return;
    const { bar, before } = drag;
    drag = null;
    const duration = view?.duration;
    if (released && duration) {
      const general = bar === generalBar ? { durationS: duration, widthPx: bar.track.getBoundingClientRect().width } : null;
      actions.releaseDrag(before, snapMarginS(general));
    }
    if (view) renderCrop(view);
  }

  function listenToBar(bar: ClipBar): void {
    bar.track.addEventListener("pointerdown", (event) => {
      const clipRange = view?.clipRange;
      const value = valueIn(bar, event.clientX);
      if (drag !== null || !clipRange || value === null || crop.getAttribute("aria-disabled") === "true") return;
      event.preventDefault();
      bar.track.setPointerCapture(event.pointerId);
      const onHandle = event.target === bar.startHandle || event.target === bar.endHandle;
      const inside = value > clipRange.start && value < clipRange.end;
      const whole =
        bar === generalBar && dragsWholeRange(clipRange, view?.duration ?? 0, bar.track.getBoundingClientRect().width);
      if (whole || (!onHandle && inside)) {
        // Touched outside a narrow range, it jumps there centred on the finger.
        const grab = inside ? value - clipRange.start : (clipRange.end - clipRange.start) / 2;
        drag = { bar, pointerId: event.pointerId, handle: null, grab, before: clipRange };
        if (!inside) actions.moveRange(value - grab);
        return;
      }
      let handle: Handle;
      if (event.target === bar.startHandle) handle = "start";
      else if (event.target === bar.endHandle) handle = "end";
      else handle = Math.abs(value - clipRange.start) <= Math.abs(value - clipRange.end) ? "start" : "end";
      drag = { bar, pointerId: event.pointerId, handle, grab: 0, before: clipRange };
      (handle === "start" ? bar.startHandle : bar.endHandle).focus();
      move(handle, value);
    });
    bar.track.addEventListener("pointermove", (event) => {
      if (drag === null || drag.bar !== bar || drag.pointerId !== event.pointerId) return;
      const value = valueIn(bar, event.clientX);
      if (value === null) return;
      if (drag.handle === null) actions.moveRange(value - drag.grab);
      else move(drag.handle, value);
    });
    bar.track.addEventListener("pointerup", (event) => endDrag(event.pointerId, true));
    bar.track.addEventListener("pointercancel", (event) => endDrag(event.pointerId, false));
    bar.track.addEventListener("lostpointercapture", (event) => endDrag(event.pointerId, false));
    bar.startHandle.addEventListener("keydown", (event) => keyboard("start", event));
    bar.endHandle.addEventListener("keydown", (event) => keyboard("end", event));
  }

  listenToBar(generalBar);
  listenToBar(zoomedBar);

  function keyboard(handle: Handle, event: KeyboardEvent): void {
    const clipRange = view?.clipRange;
    const duration = view?.duration;
    if (!clipRange || !duration) return;
    const current = handle === "start" ? clipRange.start : clipRange.end;
    const step = event.shiftKey ? LARGE_STEP_S : STEP_S;
    const keyDestinations: Record<string, number> = {
      ArrowLeft: current - step,
      ArrowDown: current - step,
      ArrowRight: current + step,
      ArrowUp: current + step,
      PageDown: current - LARGE_STEP_S,
      PageUp: current + LARGE_STEP_S,
      Home: 0,
      End: duration,
    };
    const destination = keyDestinations[event.key];
    if (destination === undefined) return;
    event.preventDefault();
    move(handle, destination);
  }
  nudges.addEventListener("click", (event) => {
    const button = event.target instanceof Element ? event.target.closest<HTMLButtonElement>("button[data-nudge]") : null;
    const clipRange = view?.clipRange;
    if (!button || !clipRange || creating()) return;
    const delta = Number(button.dataset.direction) * NUDGE_S;
    if (button.dataset.nudge === "start") actions.moveStart(clipRange.start + delta);
    else actions.moveEnd(clipRange.end + delta);
  });
  durations.addEventListener("click", (event) => {
    const choice = pressedButton(event)?.dataset["duration"];
    if (choice === undefined || view === null || creating()) return;
    actions.pickDuration(Number(choice));
  });
  autoFitButton.addEventListener("click", () => actions.autoFit());

  function renderDestinations(v: EditorView): void {
    destinations.hidden = v.presets.length === 0;
    if (paintedPresets !== v.presets) {
      paintedPresets = v.presets;
      destinations.replaceChildren(
        ...v.presets.map((preset) => {
          const card = create("label", "destination");
          const entry = create("input", "");
          entry.type = "radio";
          entry.name = "destination";
          entry.value = preset.name;
          const name = create("span", "destination-title", preset.title);
          const data = create(
            "span",
            "destination-data",
            [formatLabel(preset.extension), `${preset.side} px`, sizeText(preset.max_size_bytes), shortSecondsText(preset.max_duration_s)].join(" · "),
          );
          const mark = create("span", "destination-mark");
          mark.setAttribute("aria-hidden", "true");
          mark.append(icon("ok"));
          card.append(entry, name, data, mark);
          return card;
        }),
      );
    }
    for (const entry of destinations.querySelectorAll<HTMLInputElement>("input[type=radio]")) {
      entry.checked = entry.value === v.preset?.name;
      entry.disabled = creating();
    }
  }

  function renderPreviewMessage(): void {
    const v = view;
    if (v === null || v.entry.media.kind !== "video") return;
    const failed = v.preview.kind === "failed" || failedPreview;
    preview.dataset.state = failed ? "failed" : currentUrl ? "ready" : "loading";
    if (failed) {
      const reason =
        v.preview.kind === "failed" ? v.preview.message : playableNoVideoReason(platform);
      previewMessage.textContent = `No se puede previsualizar este video (${reason}). La barra de recorte sigue funcionando.`;
    } else {
      previewMessage.textContent = "Preparando la vista previa…";
    }
  }

  function renderStill(v: EditorView): void {
    if (v.still === null) {
      image.removeAttribute("src");
      preview.dataset.state = "loading";
      previewMessage.textContent = "Preparando la vista previa…";
      return;
    }
    if (image.getAttribute("src") !== v.still) image.src = v.still;
    image.alt = "Imagen fija del inicio del tramo";
    preview.dataset.state = "ready";
  }

  function renderVideo(v: EditorView): void {
    if (stillOnly) {
      renderStill(v);
      return;
    }
    video.defaultPlaybackRate = v.speed;
    video.playbackRate = v.speed;
    const url = v.preview.kind === "ready" ? v.preview.url : null;
    if (v.preview.kind === "ready") fromPreview = v.preview.fromS;
    if (url !== currentUrl) {
      currentUrl = url;
      playingWhole = false;
      failedPreview = false;
      previousClipRange = null;
      if (currentUrl) {
        video.src = currentUrl;
        play();
      } else {
        video.removeAttribute("src");
        video.load();
      }
    }
    const clipRange = v.clipRange;
    if (previousClipRange && (clipRange?.start !== previousClipRange.start || clipRange.end !== previousClipRange.end)) {
      playingWhole = false;
    }
    if (currentUrl && clipRange && !failedPreview) {
      if (previousClipRange && clipRange.end !== previousClipRange.end && clipRange.start === previousClipRange.start) {
        video.currentTime = secondInPreview(Math.max(clipRange.start, clipRange.end - 1), fromPreview);
      } else if (!previousClipRange || clipRange.start !== previousClipRange.start) {
        video.currentTime = secondInPreview(clipRange.start, fromPreview);
      }
    }
    previousClipRange = clipRange;
    renderPreviewMessage();
  }

  function renderImage(v: EditorView): void {
    if (v.image === null) {
      image.removeAttribute("src");
      preview.dataset.state = "loading";
      previewMessage.textContent = "Preparando la imagen…";
      return;
    }
    if (image.getAttribute("src") !== v.image) {
      image.src = v.image;
      preview.dataset.state = "ready";
    }
    image.alt = v.framing.mode === "crop" ? "Imagen original; la zona clara es lo que queda" : "Imagen entera con borde transparente, como quedará";
  }

  function renderFraming(v: EditorView): void {
    const size = sourceSize();
    if (size === null) return;
    const layout = { ...fitLayout(v.framing, size.width, size.height) };
    const crop = v.framing.mode === "crop";
    // Easy mode shows the square that will be produced: the cropped area fills the canvas.
    const resultOnly = crop && v.mode === "easy";
    if (resultOnly) {
      const area = cropSquare(v.framing, size.width, size.height);
      layout.left = -area.left / area.width;
      layout.top = -area.top / area.height;
      layout.width = 1 / area.width;
      layout.height = 1 / area.height;
    }
    previewFrame.style.inset = "auto";
    previewFrame.style.left = `${layout.left * 100}%`;
    previewFrame.style.top = `${layout.top * 100}%`;
    previewFrame.style.width = `${layout.width * 100}%`;
    previewFrame.style.height = `${layout.height * 100}%`;
    preview.dataset.framing = v.framing.mode;
    const background = v.framing.background;
    preview.dataset.background = crop ? "transparent" : background.kind;
    preview.style.setProperty("--preview-colour", background.kind === "color" ? background.rgb : "transparent");
    preview.style.setProperty("--preview-blur", v.image !== null ? `url("${v.image}")` : "none");
    const radius = shapeRadius(v.framing.shape);
    const content = contentBox(v.framing);
    previewContent.style.left = `${content.left * 100}%`;
    previewContent.style.top = `${content.top * 100}%`;
    previewContent.style.width = `${content.size * 100}%`;
    previewContent.style.height = `${content.size * 100}%`;
    previewContent.style.borderRadius = crop ? "0" : radius;
    square.style.borderRadius = crop ? radius : "0";
    square.hidden = !crop || resultOnly;
    // A huge box-shadow may not be painted on software GL, so the still itself is dimmed and the
    // square redraws the same image at full brightness, aligned with the frame.
    const dimmed = stillOnly && v.still !== null && crop && !resultOnly;
    preview.dataset.dim = String(dimmed);
    square.style.backgroundImage = dimmed ? `url("${v.still}")` : "";
    if (crop) {
      const box = cropSquare(v.framing, size.width, size.height);
      square.style.left = `${box.left * 100}%`;
      square.style.top = `${box.top * 100}%`;
      square.style.width = `${box.width * 100}%`;
      square.style.height = `${box.height * 100}%`;
      if (dimmed) {
        square.style.backgroundSize = `${100 / box.width}% ${100 / box.height}%`;
        const free = (side: number) => (side >= 1 ? 0 : 100 / (1 - side));
        square.style.backgroundPosition = `${box.left * free(box.width)}% ${box.top * free(box.height)}%`;
      }
    }
    for (const entry of framingControls.querySelectorAll<HTMLInputElement>("input[type=radio]")) {
      const current: Record<string, string> = {
        framing: v.framing.mode,
        background: background.kind,
        shape: v.framing.shape,
        align: v.framing.align,
      };
      entry.checked = entry.value === current[entry.name];
      entry.disabled = creating();
    }
    fitBox.hidden = crop;
    colourBox.hidden = crop || background.kind !== "color";
    if (background.kind === "color") {
      colourCustom.value = background.rgb;
      colourCustom.disabled = creating();
    }
    for (const swatch of swatches.querySelectorAll<HTMLButtonElement>("button")) {
      swatch.setAttribute("aria-pressed", String(background.kind === "color" && background.rgb === swatch.dataset.rgb));
      swatch.disabled = creating();
    }
    for (const chip of scaleChips.querySelectorAll<HTMLButtonElement>("button")) {
      chip.setAttribute("aria-pressed", String(v.framing.scale === Number(chip.dataset.scale)));
      chip.disabled = creating();
    }
    framingExplanation.textContent = crop
      ? "Arrastra el cuadrado sobre la vista previa para elegir qué se queda."
      : "Se ve el cuadro entero; el resto del lienzo queda con el fondo que elijas.";
    zoomBox.hidden = !crop;
    if (document.activeElement !== zoom || Number(zoom.value) !== v.framing.zoom) zoom.value = String(v.framing.zoom);
    zoom.disabled = creating();
    zoom.setAttribute("aria-valuetext", zoomText(v.framing.zoom));
    zoomValue.textContent = zoomText(v.framing.zoom);
    resetFraming.disabled = creating() || framingResetText(v.framing) === null;
  }

  function renderHandle(handle: HTMLDivElement, value: number, span: BarSpan, min: number, max: number): void {
    handle.style.left = `${barFraction(value, span) * 100}%`;
    handle.setAttribute("aria-valuemin", String(min));
    handle.setAttribute("aria-valuemax", String(max));
    handle.setAttribute("aria-valuenow", String(value));
    handle.setAttribute("aria-valuetext", secondsText(value));
  }

  function renderThumbnails(urls: readonly string[]): void {
    if (thumbs.dataset.painted === urls.join("|")) return;
    thumbs.dataset.painted = urls.join("|");
    thumbs.replaceChildren(
      ...urls.map((url) => {
        const frame = document.createElement("img");
        frame.src = url;
        frame.alt = "";
        frame.draggable = false;
        return frame;
      }),
    );
  }

  function renderBar(bar: ClipBar, span: BarSpan, clipRange: ClipRange, duration: number, longest: number | null): void {
    bar.span = span;
    const percent = (second: number): number => barFraction(second, span) * 100;
    bar.clipRange.style.left = `${percent(clipRange.start)}%`;
    bar.clipRange.style.width = `${percent(clipRange.end) - percent(clipRange.start)}%`;
    const region = longest === null ? null : shadedRegion(clipRange, longest, duration);
    bar.shade.hidden = region === null;
    if (region !== null) {
      const left = percent(region.left * duration);
      bar.shade.style.left = `${left}%`;
      bar.shade.style.width = `${percent((region.left + region.width) * duration) - left}%`;
    }
    renderHandle(bar.startHandle, clipRange.start, span, 0, clipRange.end);
    renderHandle(bar.endHandle, clipRange.end, span, clipRange.start, duration);
  }

  function renderZoomedBar(clipRange: ClipRange, duration: number, longest: number | null): void {
    // While dragging, the window stays still so the pointer keeps meaning the same seconds.
    const span = drag !== null && zoomedBar.span !== null ? zoomedBar.span : zoomSpan(clipRange, duration);
    zoomBar.hidden = span === null;
    if (span === null) {
      zoomedBar.span = null;
      return;
    }
    renderBar(zoomedBar, span, clipRange, duration, longest);
    zoomFrom.textContent = secondsText(span.from);
    zoomTo.textContent = secondsText(span.to);
  }

  function renderDurations(v: EditorView, active: boolean): void {
    const recommended = editorRecommendedS(v);
    for (const button of durations.querySelectorAll<HTMLButtonElement>("button[data-duration]")) {
      const seconds = Number(button.dataset["duration"]);
      button.disabled =
        !active ||
        v.preset === null ||
        v.duration === null ||
        !durationFits(seconds, v.duration, barScope(v.preset.max_duration_s));
    }
    const fitted =
      v.clipRange !== null && recommended !== null && Math.abs(clipRangeDuration(v.clipRange) - recommended) < 0.05;
    autoFitButton.textContent = recommended === null ? "Autoajuste" : `Autoajuste (${shortSecondsText(recommended)})`;
    autoFitButton.disabled = !active || recommended === null || fitted;
  }

  function renderPlayback(v: EditorView): void {
    playbackButtons.hidden = stillOnly;
    if (stillOnly) {
      showMessage(playbackNote, "Vista previa sin movimiento: este dispositivo no puede mostrar el video");
      return;
    }
    const playable = currentUrl !== null && !failedPreview && v.clipRange !== null;
    playWholeButton.disabled = !playable;
    playWholeButton.textContent = playingWhole ? "Repetir el tramo" : "Reproducir todo el video";
    startHereButton.disabled = !playable || creating();
    const partial = playingWhole && previewMode(platform) === "window-webm" && (v.duration ?? 0) > PREVIEW_WINDOW_S;
    showMessage(
      playbackNote,
      partial ? `La vista previa solo abarca ${PREVIEW_WINDOW_S} s del video, alrededor del tramo.` : null,
    );
  }

  function renderCrop(v: EditorView): void {
    const { clipRange, duration } = v;
    const active = clipRange !== null && duration !== null && duration > 0 && !creating();
    crop.setAttribute("aria-disabled", String(!active));
    for (const bar of [generalBar, zoomedBar]) {
      bar.startHandle.tabIndex = bar.endHandle.tabIndex = active ? 0 : -1;
    }
    for (const button of nudges.querySelectorAll<HTMLButtonElement>("button")) button.disabled = !active;
    renderThumbnails(v.thumbnails);
    renderDurations(v, active);
    const note = v.clipNote ?? "";
    // A live region is only announced when its text changes, so it is not rewritten on every render.
    if (clipNote.textContent !== note) clipNote.textContent = note;
    if (clipRange === null || duration === null || duration <= 0) {
      cropText.textContent = "Sin video";
      cropOutput.textContent = "";
      generalBar.clipRange.style.width = "0%";
      generalBar.shade.style.width = "0%";
      generalBar.span = null;
      zoomedBar.span = null;
      zoomBar.hidden = true;
      return;
    }
    const longest = maxRangeS(v.preset, v.speed, readyMaxRange(v.estimate));
    renderBar(generalBar, { from: 0, to: duration }, clipRange, duration, longest);
    renderZoomedBar(clipRange, duration, longest);
    const labels = clipBarLabels(clipRange, v.speed);
    cropText.textContent = `${labels.start} → ${labels.end} · ${labels.length}`;
    cropOutput.textContent = `Inicio ${labels.start}, fin ${labels.end}, tramo ${labels.length}. El resultado dura ${labels.output}.`;
  }

  function renderSide(v: EditorView): void {
    const range = v.preset?.side_range_px ?? null;
    const choices = v.preset ? sideChoices(v.preset) : [];
    const sticker = v.destination === "sticker";
    sideBox.hidden = v.preset === null || (range === null && !sticker);
    sideNote.hidden = range !== null;
    sideChips.hidden = range === null;
    sideCustomBox.hidden = range === null;
    sideError.hidden = range === null || sideError.textContent === "";
    sideBox.querySelector("legend")!.textContent = range === null ? "Tamaño de salida" : "Tamaño del GIF";
    if (range === null) return;
    const key = choices.map((choice) => choice.label).join("|");
    if (sideChips.dataset.key !== key) {
      sideChips.dataset.key = key;
      sideChips.replaceChildren(
        ...choices.map((choice) => {
          const chip = create("button", "chip", choice.label);
          chip.type = "button";
          chip.dataset.side = choice.value === null ? "" : String(choice.value);
          return chip;
        }),
      );
    }
    for (const chip of sideChips.querySelectorAll<HTMLButtonElement>("button")) {
      chip.setAttribute("aria-pressed", String((chip.dataset.side === "" ? null : Number(chip.dataset.side)) === v.sidePx));
      chip.disabled = creating();
    }
    sideCustom.disabled = creating();
    if (document.activeElement !== sideCustom) {
      sideCustom.value = v.sidePx !== null && !range.suggested.includes(v.sidePx) ? String(v.sidePx) : "";
    }
    sideCustom.setAttribute("aria-label", `Otro tamaño, entre ${range.min} y ${range.max} píxeles`);
  }

  function renderVideoControls(v: EditorView): void {
    renderCrop(v);
    if (document.activeElement !== speed || Number(speed.value) !== v.speed) {
      speed.value = String(v.speed);
    }
    speed.disabled = creating();
    speed.setAttribute("aria-valuetext", speedText(v.speed));
    speedValue.textContent = speedText(v.speed);
    speedDetail.textContent = v.clipRange ? outputDurationText(clipRangeDuration(v.clipRange), v.speed) : "";
    fitButton.disabled = creating() || v.clipRange === null || v.preset === null;
    showMessage(fittingNotice, v.fitting);
    for (const entry of priority.querySelectorAll<HTMLInputElement>("input")) {
      entry.checked = entry.value === v.priority;
      entry.disabled = creating();
    }
    const choices = fpsChoices(v.preset);
    for (const entry of fpsControl.querySelectorAll<HTMLInputElement>("input[type=radio]")) {
      const value = entry.value === "auto" ? null : Number(entry.value);
      entry.closest("label")!.hidden = !choices.includes(value);
      entry.checked = value === v.fps;
      entry.disabled = creating();
    }
    fpsCustom.disabled = creating();
    if (document.activeElement !== fpsCustom) fpsCustom.value = v.fps !== null && !choices.includes(v.fps) ? String(v.fps) : "";
    renderSide(v);
    priorityExplanation.textContent = priorityText(v.priority).explanation;
  }

  function renderEstimate(v: EditorView, reason: string | null): void {
    estimateDetail.hidden = true;
    const e0 = v.estimate;
    const tooLong = v.entry.media.kind === "video" && reason !== null && durationFix(v) !== null;
    const notFitting =
      tooLong || (v.entry.media.kind === "video" && reason === null && e0.kind === "ready" && !e0.estimate.fits);
    makeFitButton.hidden = !notFitting && !v.fitPending;
    makeFitButton.disabled = creating() || v.fitPending;
    makeFitButton.textContent = v.fitPending ? "Ajustando…" : "Hacer que quepa";
    if (e0.kind === "ready" && v.entry.media.kind === "video") showMessage(estimateDetail, detailText(e0.estimate));
    if (v.entry.media.kind === "image") {
      estimate.dataset.state = "image";
      estimatedSize.textContent = "Sticker estático: 512 × 512 px, como mucho 100 KB.";
      estimatedSeconds.textContent = "";
      return;
    }
    const e = v.estimate;
    estimate.dataset.state = e.kind === "ready" ? (e.estimate.fits ? "cabe" : "does-not-fit") : e.kind;
    if (reason !== null) {
      estimate.dataset.state = "does-not-fit";
      estimatedSize.replaceChildren(icon("notice"), document.createTextNode(reason));
      estimatedSeconds.textContent = "";
      return;
    }
    switch (e.kind) {
      case "without-request":
        estimatedSize.textContent = "";
        estimatedSeconds.textContent = "";
        break;
      case "calculating":
        estimatedSize.textContent = "Calculando el peso…";
        estimatedSeconds.textContent = "";
        break;
      case "ready":
        estimatedSize.replaceChildren(
          ...(e.estimate.fits ? [] : [icon("notice")]),
          document.createTextNode(estimateText(e.estimate)),
        );
        estimatedSeconds.textContent = maxSecondsText(e.estimate);
        break;
      case "failed":
        estimatedSize.textContent = `No se pudo estimar el peso (${e.message}). Puedes crear igualmente.`;
        estimatedSeconds.textContent = "";
        break;
    }
  }

  function announce(text: string): void {
    if (text === announcementLast) return;
    announcementLast = text;
    announcement.textContent = text;
  }

  function renderCreate(v: EditorView, reason: string | null): void {
    const c = v.creation;
    const inCourse = c.kind === "creating";
    createButton.dataset.creating = String(inCourse);
    const easy = v.mode === "easy";
    const blocked = easy ? reason !== null && durationFix(v) === null : reason !== null;
    createButton.disabled = !inCourse && blocked;
    if (inCourse || v.preparing) createButton.setAttribute("aria-disabled", "true");
    else createButton.removeAttribute("aria-disabled");
    cancelButton.hidden = !inCourse;
    createWithSoundButton.hidden = !canSaveClipVideo(platform, v.destination, v.entry.media);
    createWithSoundButton.disabled = v.clipRange === null || v.clipVideo.kind === "saving" || v.botSend.kind === "running";
    clipVideoState.hidden = v.clipVideo.kind === "idle";
    clipVideoState.classList.toggle("message-error", v.clipVideo.kind === "failed");
    clipVideoState.textContent = clipVideoText(v.clipVideo);
    soundBox.hidden = v.sound.kind === "unavailable" || v.clipRange === null;
    soundButton.disabled = v.sound.kind === "saving" || inCourse;
    soundState.classList.toggle("message-error", v.sound.kind === "failed");
    soundState.textContent = soundText(v.sound);
    clipVideoFolder.hidden = !folderButtonVisible(platform, v.clipVideo.kind === "saved" ? "videos" : null);
    openWhatsappButton.hidden = !openWhatsappVisible(platform, v.clipVideo);
    const targets = manualTargets(sendTargets(v.extensions, platform, v.clipVideo), v.bot);
    extensionSends.replaceChildren(
      ...targets.map(({ extension }) => {
        const button = create("button", "button-text", `Enviar a ${extension.name}`);
        button.type = "button";
        button.dataset["extension"] = extension.id;
        return button;
      }),
    );
    const soundS = v.clipRange === null ? 0 : v.clipRange.end - v.clipRange.start;
    extensionNotices.replaceChildren(
      ...targets
        .flatMap(({ extension }) => [soundDurationNotice(extension, soundS), stillImageNotice(extension, soundS)])
        .flatMap((text) => text ?? [])
        .map((text) => create("p", "secondary small", text)),
    );
    renderBot(v, inCourse);
    soundFolder.hidden = !folderButtonVisible(platform, v.sound.kind === "saved" ? "sounds" : null);
    editorPanel.dataset["mode"] = v.mode;
    for (const entry of modeSwitch.querySelectorAll<HTMLInputElement>("input")) entry.checked = entry.value === v.mode;
    modeSwitch.disabled = inCourse;
    showMessage(easyNote, v.easyNote);

    if (inCourse) {
      const fraction = progressFraction(c.progress);
      createButton.dataset.indeterminate = String(fraction === null);
      createBar.style.width = fraction === null ? "" : `${fraction * 100}%`;
      createText.textContent = c.cancelling ? "Cancelando…" : progressText(c.progress);
      cancelButton.disabled = c.cancelling;
      announce(c.cancelling ? "Cancelando" : `Creando: ${progressText(c.progress)}`);
    } else {
      createButton.dataset.indeterminate = "false";
      createBar.style.width = "0";
      const label = easy ? (v.destination === "gif" ? "Crear GIF" : "Crear sticker") : "Crear";
      createText.textContent = v.preparing ? "Ajustando para que quepa…" : c.kind === "done" ? "Crear otra vez" : label;
    }

    createError.hidden = c.kind !== "error";
    if (c.kind === "error") {
      createErrorText.textContent = errorMessage(c.error);
      announce(`Error: ${errorMessage(c.error)}`);
    }
    result.hidden = c.kind !== "done";
    editorPanel.dataset["bar"] = bottomAction(platform, "editor", c.kind === "done");
    if (c.kind === "done") renderResult(v, c);
  }

  /** Only on a real change, so screen readers do not repeat it every time the bot is asked. */
  function setText(node: HTMLElement, text: string): void {
    if (node.textContent !== text) node.textContent = text;
  }

  function renderBot(v: EditorView, creating: boolean): void {
    const box = botBox(v.destination, v.entry.media, v.extensions, v.bot);
    botBoxNode.hidden = box === null;
    if (box === null) return;
    setText(botLine, box.line);
    setText(botOpenButton, box.panelLabel);
    botOpenButton.disabled = creating || v.botSend.kind === "running";
    const send = box.send;
    botSendButton.hidden = send === null;
    botSendButton.disabled =
      send?.reason != null || creating || v.clipRange === null || v.clipVideo.kind === "saving" || v.botSend.kind === "running";
    const state = v.botSend;
    const hint = "Deja el tramo en 10 s como máximo, activa el bot si está en pausa, guarda el clip y lo envía.";
    const note = state.kind === "running" ? state.text : state.kind === "done" ? state.message : (send?.reason ?? hint);
    setText(botSendNote, send === null && state.kind === "idle" ? "" : note);
    botSendNote.classList.toggle("message-error", state.kind === "done" && !state.sent);
    const caption = state.kind === "done" ? state.caption : null;
    botCaptionBox.hidden = caption === null;
    if (caption !== botCaption.textContent) setText(botCaptionCopied, "");
    setText(botCaption, caption ?? "");
  }

  function renderAdd(v: EditorView, c: Extract<Creation, { kind: "done" }>): void {
    const esSticker = v.destination === "sticker" && c.result.meets_limit;
    const addedAlready = c.addedTo !== null;
    const { open, compatible } = v.packs;
    openAddButton.hidden = !esSticker || addedAlready || open === null;
    if (open !== null) openAddButton.textContent = `Añadir a «${open.name}»`;
    openAddButton.disabled = c.adding;
    addPick.hidden = !esSticker || addedAlready || open !== null || compatible.length === 0;
    const value = packSelector.value;
    packSelector.replaceChildren(...compatible.map((p) => new Option(p.name, String(p.id))));
    if (compatible.some((p) => String(p.id) === value)) packSelector.value = value;
    addButton.disabled = c.adding;
    withoutPacks.hidden = !esSticker || addedAlready || open !== null || compatible.length > 0;
    const kind = v.entry.media.kind === "image" ? "estáticos" : "animated";
    withoutPacks.textContent = `No tienes ningún paquete de stickers ${kind} con hueco. Crea uno en la pestaña Stickers.`;
    addState.classList.toggle("message-error", c.addError !== null);
    addState.replaceChildren(
      ...(addedAlready ? [icon("ok")] : []),
      document.createTextNode(
        c.addError ?? (c.adding ? "Añadiendo…" : addedAlready ? `Añadido a «${c.addedTo}»` : ""),
      ),
    );
    saveButton.classList.toggle("button-main", v.destination === "gif");
    saveButton.classList.toggle("button-secondary", v.destination !== "gif");
  }

  function renderResult(v: EditorView, c: Extract<Creation, { kind: "done" }>): void {
    const r = c.result;
    result.dataset.meets_limit = String(r.meets_limit);
    resultTitle.replaceChildren(
      document.createTextNode(sizeText(r.size_bytes)),
      create("span", "", ` de ${sizeText(r.max_size_bytes)}`),
    );
    resultMeets.replaceChildren(
      icon(r.meets_limit ? "ok" : "notice"),
      document.createTextNode(r.meets_limit ? "Cumple las reglas del destino" : "No cumple: pesa más de lo permitido"),
    );
    detail.textContent = r.meets_limit ? resultDetail(r) : `Lo más ligero que se consiguió: ${resultSizeText(r)}`;
    showMessage(notice, r.notice);
    if (c.image !== null) {
      if (resultImage.getAttribute("src") !== c.image) resultImage.src = c.image;
      resultImage.alt = v.destination === "gif" ? "GIF creado" : "Sticker creado";
    } else {
      resultImage.removeAttribute("src");
    }
    renderAdd(v, c);
    saveButton.hidden = !r.meets_limit;
    saveButton.disabled = c.saving;
    shareButton.hidden = !canShareResult(platform, v.destination, r.meets_limit);
    shareButton.disabled = c.saving;
    sendWhatsappButton.hidden = !canSendToWhatsapp(platform, v.destination, r.meets_limit);
    sendWhatsappButton.disabled = c.saving;
    saved.classList.toggle("message-error", c.saveError !== null);
    saved.textContent =
      c.saveError ?? (c.saving ? "Guardando…" : (c.sentNotice ?? (c.saved ? `Guardado como ${c.saved}` : "")));
    announce(r.meets_limit ? `Listo: ${resultSizeText(r)}` : `No cabe. ${r.notice ?? ""}`);
  }

  return {
    render(fresh) {
      if (fresh?.entry.id !== view?.entry.id) {
        drag = null;
        zoomedBar.span = null;
      }
      view = fresh;
      if (fresh === null) {
        if (currentUrl !== null || video.getAttribute("src")) {
          currentUrl = null;
          video.removeAttribute("src");
          video.load();
        }
        image.removeAttribute("src");
        paintedPresets = null;
        return;
      }
      const esVideo = fresh.entry.media.kind === "video";
      editorTitle.textContent = title(fresh);
      file.textContent = fresh.entry.name;
      preview.dataset.media = fresh.entry.media.kind;
      preview.dataset.still = String(stillOnly && esVideo);
      videoControls.hidden = !esVideo;
      renderDestinations(fresh);
      const reason = esVideo ? reasonForCreateNo(fresh) : null;
      if (esVideo) {
        renderVideo(fresh);
        renderVideoControls(fresh);
        renderPlayback(fresh);
      } else {
        renderImage(fresh);
      }
      renderFraming(fresh);
      renderEstimate(fresh, reason);
      renderCreate(fresh, reason);
    },
  };
}
