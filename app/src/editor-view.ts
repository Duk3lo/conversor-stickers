import { bottomAction } from "./bottom-bar.ts";
import { sizeText } from "./queue.ts";
import type { AlignUi, BackgroundUi, FramingModeUi, FramingSpecUi, PresetUi, PriorityUi, ShapeUi } from "./contract.ts";
import { NUDGE_S, clipBarLabels, maxRangeS, readyMaxRange, shadedRegion } from "./clipbar.ts";
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
import { create, element, icon, showMessage } from "./dom.ts";
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
  framingResetText,
  withZoom,
  zoomText,
} from "./framing.ts";
import { errorMessage } from "./messages.ts";
import { canSaveClipVideo, clipVideoText } from "./clip-video.ts";
import { soundText } from "./sound.ts";
import { folderButtonVisible } from "./saved-folder.ts";
import { playableNoVideoReason } from "./mobile.ts";
import { platform } from "./platform.ts";
import { canSendToWhatsapp, canShareResult } from "./share.ts";
import { secondInPreview } from "./preview.ts";
import { resultDetail, progressFraction, resultSizeText, progressText } from "./progress.ts";
import {
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
  const track = element<HTMLDivElement>("crop-track");
  const thumbs = element<HTMLDivElement>("crop-thumbs");
  const shade = element<HTMLDivElement>("crop-shade");
  const nudges = element<HTMLDivElement>("crop-nudges");
  const cropOutput = element<HTMLParagraphElement>("crop-output");
  const fitTimeButton = element<HTMLButtonElement>("fit-time");
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
  const trackClipRange = element<HTMLDivElement>("crop-clip-range");
  const startHandle = element<HTMLDivElement>("handle-start");
  const endHandle = element<HTMLDivElement>("handle-end");
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
  soundFolder.addEventListener("click", () => actions.openFolder("sounds"));
  shareButton.addEventListener("click", () => actions.share());
  sendWhatsappButton.addEventListener("click", () => actions.sendToWhatsapp());
  openAddButton.addEventListener("click", () => {
    const open = view?.packs.open;
    if (open) actions.addPack(open.id);
  });
  addButton.addEventListener("click", () => {
    if (packSelector.value !== "") actions.addPack(Number(packSelector.value));
  });

  function keepInClipRange(): void {
    const clipRange = view?.clipRange;
    if (!clipRange || !currentUrl || failedPreview) return;
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
    goStartOfClipRange();
    play();
  });
  video.addEventListener("error", () => {
    if (!currentUrl) return;
    failedPreview = true;
    console.warn("el video no se puede previsualizar", video.error?.message ?? "");
    renderPreviewMessage();
  });
  image.addEventListener("error", () => {
    if (image.getAttribute("src")) {
      preview.dataset.state = "failed";
      previewMessage.textContent = "No se puede mostrar esta imagen, pero se puede convertir igualmente.";
    }
  });

  type Handle = "start" | "end";
  let dragging: Handle | null = null;
  let movingRange: { grab: number } | null = null;

  function valueIn(clientX: number): number | null {
    const duration = view?.duration;
    if (!duration) return null;
    const box = track.getBoundingClientRect();
    const fraction = Math.min(Math.max((clientX - box.left) / box.width, 0), 1);
    return fraction * duration;
  }

  function move(handle: Handle, seconds: number): void {
    if (handle === "start") actions.moveStart(seconds);
    else actions.moveEnd(seconds);
  }

  track.addEventListener("pointerdown", (event) => {
    const clipRange = view?.clipRange;
    const value = valueIn(event.clientX);
    if (!clipRange || value === null || crop.getAttribute("aria-disabled") === "true") return;
    event.preventDefault();
    if (event.target !== startHandle && event.target !== endHandle && value > clipRange.start && value < clipRange.end) {
      movingRange = { grab: value - clipRange.start };
      track.setPointerCapture(event.pointerId);
      return;
    }
    if (event.target === startHandle) dragging = "start";
    else if (event.target === endHandle) dragging = "end";
    else dragging = Math.abs(value - clipRange.start) <= Math.abs(value - clipRange.end) ? "start" : "end";
    (dragging === "start" ? startHandle : endHandle).focus();
    track.setPointerCapture(event.pointerId);
    move(dragging, value);
  });
  track.addEventListener("pointermove", (event) => {
    if (movingRange !== null) {
      const value = valueIn(event.clientX);
      if (value !== null) actions.moveRange(value - movingRange.grab);
      return;
    }
    if (dragging === null) return;
    const value = valueIn(event.clientX);
    if (value !== null) move(dragging, value);
  });
  const drop = (): void => {
    dragging = null;
    movingRange = null;
  };
  track.addEventListener("pointerup", drop);
  track.addEventListener("pointercancel", drop);

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
  fitTimeButton.addEventListener("click", () => actions.fitToTime());
  startHandle.addEventListener("keydown", (event) => keyboard("start", event));
  endHandle.addEventListener("keydown", (event) => keyboard("end", event));

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

  function renderVideo(v: EditorView): void {
    video.defaultPlaybackRate = v.speed;
    video.playbackRate = v.speed;
    const url = v.preview.kind === "ready" ? v.preview.url : null;
    if (v.preview.kind === "ready") fromPreview = v.preview.fromS;
    if (url !== currentUrl) {
      currentUrl = url;
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
    if (crop) {
      const box = cropSquare(v.framing, size.width, size.height);
      square.style.left = `${box.left * 100}%`;
      square.style.top = `${box.top * 100}%`;
      square.style.width = `${box.width * 100}%`;
      square.style.height = `${box.height * 100}%`;
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

  function renderHandle(handle: HTMLDivElement, value: number, duration: number, min: number, max: number): void {
    handle.style.left = `${(value / duration) * 100}%`;
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

  function renderCrop(v: EditorView): void {
    const { clipRange, duration } = v;
    const active = clipRange !== null && duration !== null && duration > 0 && !creating();
    crop.setAttribute("aria-disabled", String(!active));
    for (const handle of [startHandle, endHandle]) handle.tabIndex = active ? 0 : -1;
    for (const button of nudges.querySelectorAll<HTMLButtonElement>("button")) button.disabled = !active;
    renderThumbnails(v.thumbnails);
    if (clipRange === null || duration === null || duration <= 0) {
      cropText.textContent = "Sin video";
      cropOutput.textContent = "";
      trackClipRange.style.width = "0%";
      shade.style.width = "0%";
      fitTimeButton.disabled = true;
      return;
    }
    trackClipRange.style.left = `${(clipRange.start / duration) * 100}%`;
    trackClipRange.style.width = `${((clipRange.end - clipRange.start) / duration) * 100}%`;
    const longest = maxRangeS(v.preset, v.speed, readyMaxRange(v.estimate));
    const region = longest === null ? null : shadedRegion(clipRange, longest, duration);
    shade.hidden = region === null;
    if (region !== null) {
      shade.style.left = `${region.left * 100}%`;
      shade.style.width = `${region.width * 100}%`;
    }
    renderHandle(startHandle, clipRange.start, duration, 0, clipRange.end);
    renderHandle(endHandle, clipRange.end, duration, clipRange.start, duration);
    const labels = clipBarLabels(clipRange, v.speed);
    cropText.textContent = `${labels.start} → ${labels.end} · ${labels.length}`;
    cropOutput.textContent = `Inicio ${labels.start}, fin ${labels.end}, tramo ${labels.length}. El resultado dura ${labels.output}.`;
    const same = longest !== null && Math.abs(clipRangeDuration(clipRange) - Math.floor(longest * 10 + 1e-6) / 10) < 0.05;
    fitTimeButton.disabled = !active || longest === null || same;
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
    createWithSoundButton.disabled = v.clipRange === null || v.clipVideo.kind === "saving";
    clipVideoState.hidden = v.clipVideo.kind === "idle";
    clipVideoState.classList.toggle("message-error", v.clipVideo.kind === "failed");
    clipVideoState.textContent = clipVideoText(v.clipVideo);
    soundBox.hidden = v.sound.kind === "unavailable" || v.clipRange === null;
    soundButton.disabled = v.sound.kind === "saving" || inCourse;
    soundState.classList.toggle("message-error", v.sound.kind === "failed");
    soundState.textContent = soundText(v.sound);
    clipVideoFolder.hidden = !folderButtonVisible(platform, v.clipVideo.kind === "saved" ? "videos" : null);
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
      videoControls.hidden = !esVideo;
      renderDestinations(fresh);
      const reason = esVideo ? reasonForCreateNo(fresh) : null;
      if (esVideo) {
        renderVideo(fresh);
        renderVideoControls(fresh);
      } else {
        renderImage(fresh);
      }
      renderFraming(fresh);
      renderEstimate(fresh, reason);
      renderCreate(fresh, reason);
    },
  };
}
