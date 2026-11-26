import { addEntries, emptyQueue, startDownloads } from "./queue.ts";
import { createKlipySection, type KlipySection } from "./klipy-section.ts";
import { createCatalogSection, type CatalogSection } from "./catalog-section.ts";
import { createBackupSection } from "./backup-section.ts";
import { createExtensionsSection } from "./extensions-section.ts";
import { createDetailSection, type DetailSection } from "./detail-section.ts";
import { createFavoritesSection, type FavoritesSection } from "./favorites-section.ts";
import { platform } from "./platform.ts";
import { demoLibrary, createCatalogDemo } from "./catalog-demo.ts";
import { createGifsDemo, type GifsMode } from "./demo-gifs.ts";
import { createGifSearchSection, type GifSearchSection } from "./gif-search-section.ts";
import type { Context } from "./context.ts";
import type { LibraryUi, EntryUi, PresetUi } from "./contract.ts";
import { openPack, changeSection, goTo, goToRoot, type Navigation, initialNavigation, type Root, type Path } from "./navigation.ts";
import { createDownloadView } from "./download-view.ts";
import { createEditorView } from "./editor-view.ts";
import { createGifsView } from "./gifs-view.ts";
import { createShellView } from "./shell-view.ts";
import type { SavedFolder } from "./saved-folder.ts";
import { DEFAULT_FRAMING } from "./framing.ts";
import { createStickersView } from "./stickers-view.ts";
import type { DownloadView, EditorView, GifsView, StickersView } from "./views.ts";

const presets: PresetUi[] = [
  { name: "whatsapp-sticker", title: "Sticker animado de WhatsApp", extension: "webp", side: 512, max_size_bytes: 500_000, max_duration_s: 10, max_fps: 30, side_range_px: null, fps_range: { min: 5, max: 30 } },
  { name: "discord-gif", title: "GIF para Discord", extension: "gif", side: 480, max_size_bytes: 10_000_000, max_duration_s: 30, max_fps: 30, side_range_px: { min: 64, max: 480, suggested: [240, 320, 480] }, fps_range: { min: 5, max: 30 } },
  { name: "discord-emoji", title: "Emoji animado de Discord", extension: "gif", side: 128, max_size_bytes: 256_000, max_duration_s: 10, max_fps: 30, side_range_px: null, fps_range: { min: 5, max: 30 } },
  { name: "discord-sticker", title: "Sticker de Discord", extension: "png", side: 320, max_size_bytes: 512_000, max_duration_s: 5, max_fps: 30, side_range_px: null, fps_range: { min: 5, max: 30 } },
];
const demoThumbnails = Array.from({ length: 10 }, (_, index) => {
  const hue = 260 - index * 18;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="80" height="45"><rect width="80" height="45" fill="hsl(${hue} 55% 45%)"/><circle cx="${10 + index * 6}" cy="22" r="10" fill="hsl(${hue} 70% 75%)"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
});
const [sticker, ...discord] = presets as [PresetUi, ...PresetUi[]];

const Colors = ["#8b5cf6", "#22c55e", "#f59e0b", "#06b6d4", "#ef4444", "#ec4899", "#84cc16", "#3b82f6"];

async function urlOfCanvas(canvas: HTMLCanvasElement): Promise<string> {
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve));
  return blob ? URL.createObjectURL(blob) : "";
}

async function stickerOfExample(index: number): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = 128;
  canvas.height = 128;
  const context = canvas.getContext("2d");
  if (context === null) return "";
  context.fillStyle = Colors[index % Colors.length] ?? "#8b5cf6";
  context.beginPath();
  if (index % 3 === 0) context.arc(64, 64, 52, 0, Math.PI * 2);
  else if (index % 3 === 1) context.roundRect(16, 16, 96, 96, 24);
  else {
    context.moveTo(64, 10);
    context.lineTo(118, 112);
    context.lineTo(10, 112);
  }
  context.fill();
  context.fillStyle = "#0f1115";
  context.beginPath();
  context.arc(50, 60, 7, 0, Math.PI * 2);
  context.arc(78, 60, 7, 0, Math.PI * 2);
  context.fill();
  return urlOfCanvas(canvas);
}

async function photoOfExample(): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = 800;
  canvas.height = 480;
  const context = canvas.getContext("2d");
  if (context === null) return "";
  const sky = context.createLinearGradient(0, 0, 0, 480);
  sky.addColorStop(0, "#312e81");
  sky.addColorStop(1, "#f59e0b");
  context.fillStyle = sky;
  context.fillRect(0, 0, 800, 480);
  context.fillStyle = "#fde68a";
  context.beginPath();
  context.arc(400, 300, 70, 0, Math.PI * 2);
  context.fill();
  context.fillStyle = "#1e1b4b";
  context.beginPath();
  context.moveTo(0, 480);
  context.lineTo(180, 260);
  context.lineTo(330, 400);
  context.lineTo(520, 230);
  context.lineTo(800, 480);
  context.fill();
  return urlOfCanvas(canvas);
}

async function videoOfExample(): Promise<string | null> {
  if (typeof MediaRecorder === "undefined" || location.search.includes("sin-video")) return null;
  const canvas = document.createElement("canvas");
  canvas.width = 320;
  canvas.height = 320;
  const context = canvas.getContext("2d");
  if (context === null || !("captureStream" in canvas)) return null;
  try {
    const recorder = new MediaRecorder(canvas.captureStream(15), { mimeType: "video/webm" });
    const pieces: Blob[] = [];
    recorder.ondataavailable = (event) => pieces.push(event.data);
    const end = new Promise<void>((follow) => (recorder.onstop = () => follow()));
    const start = performance.now();
    recorder.start();
    await new Promise<void>((follow) => {
      const draw = (): void => {
        const t = (performance.now() - start) / 2000;
        context.fillStyle = "#1e1b4b";
        context.fillRect(0, 0, 320, 320);
        context.fillStyle = "#8b5cf6";
        context.beginPath();
        context.arc(60 + 200 * t, 160 + 60 * Math.sin(t * 6), 40, 0, Math.PI * 2);
        context.fill();
        if (t < 1) requestAnimationFrame(draw);
        else follow();
      };
      draw();
    });
    recorder.stop();
    await end;
    return URL.createObjectURL(new Blob(pieces, { type: "video/webm" }));
  } catch (failure) {
    console.warn("demo: sin video de ejemplo", failure);
    return null;
  }
}

const gato: EntryUi = {
  id: 1,
  name: "gato-saltando.mp4",
  origin: "file",
  state: { kind: "ready" },
  media: { kind: "video", duration_s: 14.2, width: 1920, height: 1080, has_audio: true },
};

interface DemoState {
  nav?: Navigation;
  red?: "con-red" | "caché" | "sin-caché";
  clicks?: string[];
  notice?: string;
  stickers?: Partial<StickersView>;
  editor?: Partial<EditorView>;
  gifs?: Partial<GifsView>;
  searchGif?: { mode: GifsMode; provider?: "klipy" | "giphy" };
  download?: Partial<DownloadView>;
}

export async function startDemo(initial: string | null): Promise<void> {
  const paths = await Promise.all(Array.from({ length: 14 }, (_, index) => stickerOfExample(index)));
  const [photo, video] = await Promise.all([photoOfExample(), videoOfExample()]);
  const library: LibraryUi = demoLibrary(paths);
  const demo = createCatalogDemo(paths, library);
  const demoGifs = createGifsDemo(paths, () => demo.library());
  const emptyList: LibraryUi = { categories: [], packs: [], notice: null };

  const stickersBase: StickersView = {
    library: { kind: "ready", value: demo.library() },
    filter: { kind: "all" },
    openedPack: null,
    form: null,
    formError: null,
    confirmation: null,
    chosenSticker: null,
    busy: false,
    choosing: null,
    error: null,
    notices: [],
  };

  const editorVideo: EditorView = {
    entry: { id: gato.id, name: gato.name, media: gato.media ?? { kind: "video", duration_s: 14.2, width: 1920, height: 1080, has_audio: true } },
    destination: "sticker",
    presets: [],
    preset: sticker,
    preview: video ? { kind: "ready", url: video, fromS: 0 } : { kind: "loading", fromS: 0 },
    image: null,
    duration: 14.2,
    clipRange: { start: 0.4, end: 1.9 },
    speed: 1,
    fitting: null,
    clipNote: null,
    priority: "balance",
    fps: null,
    framing: DEFAULT_FRAMING,
    sidePx: null,
    thumbnails: demoThumbnails,
    fitPending: false,
    mode: "advanced",
    preparing: false,
    easyNote: null,
    estimate: {
      kind: "ready",
      estimate: { estimated_size_bytes: 380_000, max_size_bytes: 500_000, fits: true, fps: 15, max_seconds_s: 6.5, detail_px: 512, max_range_s: 6.5 },
    },
    creation: { kind: "inactive" },
    sound: { kind: "idle" },
    clipVideo: { kind: "idle" },
    extensions: [],
    packs: { open: { id: 1, name: "Gatos que saltan" }, compatible: [] },
  };

  const done = (changes: object = {}) => ({
    kind: "done" as const,
    result: {
      meets_limit: true,
      size_bytes: 411_200,
      max_size_bytes: 500_000,
      fps: 12,
      duration_s: 6,
      reductions: ["fps 15→12", "calidad 90→75"],
      notice: null,
    },
    image: paths[0] ?? null,
    saving: false,
    saved: null,
    saveError: null,
    sentNotice: null,
    adding: false,
    addedTo: null,
    addError: null,
    ...changes,
  });

  const imageEditor: EditorView = {
    ...editorVideo,
    entry: { id: 2, name: "captura-2026-11-22.png", media: { kind: "image", path: photo, width: 800, height: 480 } },
    preset: null,
    preview: { kind: "empty" },
    image: photo,
    duration: null,
    clipRange: null,
    estimate: { kind: "without-request" },
    packs: { open: null, compatible: [{ id: 3, name: "Fotos del verano" }, { id: 5, name: "Sin estrenar" }] },
  };

  const editorGif: EditorView = {
    ...editorVideo,
    destination: "gif",
    presets: discord,
    preset: discord[0] ?? null,
    clipRange: { start: 0, end: 1.9 },
    estimate: {
      kind: "ready",
      estimate: { estimated_size_bytes: 4_210_000, max_size_bytes: 10_000_000, fits: true, fps: 15, max_seconds_s: 30, detail_px: 512, max_range_s: 30 },
    },
    packs: { open: null, compatible: [] },
  };

  const completeQueue = startDownloads(
    addEntries(emptyQueue(), [
      gato,
      { id: 7, name: "pegado-7.png", origin: "pasted", state: { kind: "ready" }, media: { kind: "image", path: photo, width: 800, height: 480 } },
      { id: 2, name: "www.tiktok.com", origin: "link", state: { kind: "downloading", bytes: 3_400_000, total: 8_000_000 }, media: null },
      { id: 3, name: "youtu.be", origin: "link", state: { kind: "pending" }, media: null },
      {
        id: 4,
        name: "www.instagram.com",
        origin: "link",
        state: { kind: "error", error: { kind: "failed_download", message: "ERROR: [Instagram] login required" } },
        media: null,
      },
    ]),
  );

  const createdGifs: GifsView = {
    choosing: false,
    error: null,
    created: [
      { entry: 1, name: "gato-saltando.mp4", destination: "GIF para Discord", sizeBytes: 4_310_000, image: paths[0] ?? "", saving: false, saved: "gato-saltando.gif", saveError: null },
      { entry: 5, name: "baile.webm", destination: "Emoji animado de Discord", sizeBytes: 201_000, image: paths[4] ?? "", saving: false, saved: null, saveError: null },
    ],
  };

  const editing = (destination: "sticker" | "gif" = "sticker", section: Navigation["section"] = "stickers"): Navigation => ({
    ...changeSection(initialNavigation(), section),
    editor: { entry: 1, destination },
  });

  const navigateTo = (path: Path): Navigation => goTo(initialNavigation(), path);
  const inRoot = (root: Root): Navigation => goToRoot(initialNavigation(), root);

  const states: Record<string, DemoState> = {
    "Inicio": {},
    "Comunidad": { nav: inRoot("community") },
    "Comunidad · sin conexión (caché)": { nav: inRoot("community"), red: "caché" },
    "Comunidad · sin catálogo": { nav: inRoot("community"), red: "sin-caché" },
    "Inicio · sin catálogo": { red: "sin-caché" },
    "Inicio · sin clave de KLIPY": { searchGif: { mode: "without-key" } },
    "Inicio · KLIPY sin conexión": { searchGif: { mode: "without-connection" } },
    "Inicio · vista grande de KLIPY": { clicks: ['[data-focus="klipy-tendencias-tendencias-0"]'] },
    "Buscar · con stickers de KLIPY": { nav: goTo(initialNavigation(), { kind: "search", query: "gato" }) },
    "Detalle · propio": { nav: navigateTo({ kind: "pack", origin: "local", id: "1" }) },
    "Detalle · propio incompleto": { nav: navigateTo({ kind: "pack", origin: "local", id: "3" }) },
    "Detalle · copia retirada": { nav: navigateTo({ kind: "pack", origin: "local", id: "7" }) },
    "Detalle · comunidad": { nav: navigateTo({ kind: "pack", origin: "community", id: "perros-pixel" }) },
    "Detalle · comunidad descargada y actualizable": { nav: navigateTo({ kind: "pack", origin: "community", id: "gatos-garabato" }) },
    "Detalle · sticker de KLIPY": { nav: navigateTo({ kind: "pack", origin: "local", id: "2" }), clicks: ['[data-focus="sticker-2"]'] },
    "Detalle · sticker en grande": { nav: navigateTo({ kind: "pack", origin: "local", id: "1" }), clicks: ['[data-focus="sticker-2"]'] },
    "Detalle · selector de emoji": { nav: navigateTo({ kind: "pack", origin: "local", id: "1" }), clicks: ['[data-focus="sticker-2"]', '[data-focus="change-emoji"]'] },
    "Detalle · reordenando": { nav: navigateTo({ kind: "pack", origin: "local", id: "2" }), clicks: ['[data-focus="reorder"]'] },
    "Detalle · aviso de la comunidad": { nav: navigateTo({ kind: "pack", origin: "community", id: "perros-pixel" }), clicks: ['[data-focus="action-download"]'] },
    "Detalle · proponer a la comunidad": { nav: navigateTo({ kind: "pack", origin: "local", id: "2" }), clicks: ['[data-focus="propose"]'] },
    "Mis paquetes · copia de seguridad": { nav: inRoot("mis-packs") },
    "Favoritos": { nav: inRoot("favorites") },
    "Búsqueda · gato": { nav: navigateTo({ kind: "search", query: "gato" }) },
    "Búsqueda · sin resultados": { nav: navigateTo({ kind: "search", query: "zzzz" }) },
    "Categoría · Animales": { nav: navigateTo({ kind: "category", origin: "local", id: "4" }) },
    "Stickers · cargando": { stickers: { library: { kind: "loading" } } },
    "Stickers · vacío": { stickers: { library: { kind: "ready", value: emptyList } } },
    "Stickers · con paquetes": {},
    "Stickers · categoría y formulario": {
      stickers: { filter: { kind: "category", id: 3 }, form: { kind: "category-new" }, formError: "Ya hay una categoría «Memes»." },
    },
    "Stickers · borrar categoría": {
      stickers: { filter: { kind: "category", id: 2 }, confirmation: { kind: "delete-category", id: 2 } },
    },
    "Stickers · error al leer": {
      stickers: { library: { kind: "error", message: "sin implementar" } },
    },
    "Stickers · avisos de importar": {
      stickers: {
        notices: [
          "Importado «Reacciones» con 30 stickers.",
          "Se omitieron 5 stickers: un paquete lleva como mucho 30.",
          "No traía icono: se generó uno con el primer sticker.",
        ],
        choosing: "video",
      },
    },
    "Paquete abierto": { nav: openPack(initialNavigation(), 1), stickers: { openedPack: 1 } },
    "Paquete abierto · sticker elegido": {
      nav: openPack(initialNavigation(), 1),
      stickers: { openedPack: 1, chosenSticker: 3 },
    },
    "Paquete · no exportable y borrar": {
      nav: openPack(initialNavigation(), 3),
      stickers: { openedPack: 3, confirmation: { kind: "delete-pack", id: 3 } },
    },
    "Editor · video": { nav: editing(), editor: {} },
    "Editor · modo Fácil": { nav: editing(), editor: { mode: "easy" } },
    "Editor · video largo, modo Fácil": {
      nav: editing(),
      editor: { mode: "easy", duration: 300, clipRange: { start: 120, end: 126 }, clipNote: "Se ha vuelto al tramo original" },
    },
    "Editor · modo Fácil, ajustando": { nav: editing(), editor: { mode: "easy", preparing: true } },
    "Editor · modo Fácil, resultado": {
      nav: editing(),
      editor: {
        mode: "easy",
        easyNote: "Se aceleró a 1,6x y se bajó la calidad para que quepa.",
        creation: done(),
      },
    },
    "Editor · no cabe a esta velocidad": {
      nav: editing(),
      editor: {
        clipRange: { start: 0, end: 14.2 },
        speed: 1,
        fitting: "No entra ni a 2x: acorta el tramo 0,2 s o sube la velocidad a mano.",
      },
    },
    "Editor · cámara lenta y calculando": {
      nav: editing(),
      editor: { clipRange: { start: 0, end: 4 }, speed: 0.5, priority: "sharpness", estimate: { kind: "calculating" } },
    },
    "Editor · estimación que no cabe": {
      nav: editing(),
      editor: {
        clipRange: { start: 0, end: 8 },
        priority: "sharpness",
        estimate: {
          kind: "ready",
          estimate: { estimated_size_bytes: 742_000, max_size_bytes: 500_000, fits: false, fps: 12, max_seconds_s: 4.5, detail_px: 512, max_range_s: 4.5 },
        },
      },
    },
    "Editor · estimación fallida": {
      nav: editing(),
      editor: { estimate: { kind: "failed", message: "sin implementar" } },
    },
    "Editor · creando": {
      nav: editing(),
      editor: { creation: { kind: "creating", progress: { kind: "fitting_size", attempt: 3, max: 8 }, cancelling: false } },
    },
    "Editor · resultado": { nav: editing(), editor: { creation: done() } },
    "Editor · resultado sin paquete abierto": {
      nav: editing("sticker", "download"),
      editor: {
        creation: done({ saved: "gato-saltando.webp" }),
        packs: { open: null, compatible: [{ id: 1, name: "Gatos que saltan" }, { id: 2, name: "Reacciones" }] },
      },
    },
    "Editor · no cumple": {
      nav: editing(),
      editor: {
        creation: done({
          result: {
            meets_limit: false,
            size_bytes: 612_000,
            max_size_bytes: 500_000,
            fps: 8,
            duration_s: 6,
            reductions: [],
            notice: "No cabe en 500 KB: lo más ligero pesó 612 KB. Prueba con un tramo de 4,5 s como mucho.",
          },
          image: null,
        }),
      },
    },
    "Editor · error": {
      nav: editing(),
      editor: {
        creation: {
          kind: "error",
          error: { kind: "ffmpeg_not_found", message: "no se encontró `ffprobe`; instala FFmpeg (por ejemplo `sudo pacman -S ffmpeg`)" },
        },
      },
    },
    "Editor · imagen recortada": { nav: editing(), editor: imageEditor },
    "Editor · imagen encajada y añadida": {
      nav: editing(),
      editor: {
        ...imageEditor,
        framing: { ...DEFAULT_FRAMING, mode: "fit", shape: "rounded", scale: 0.85, background: { kind: "color", rgb: "#8b5cf6" } },
        creation: done({
          result: { meets_limit: true, size_bytes: 61_000, max_size_bytes: 100_000, fps: null, duration_s: null, reductions: [], notice: null },
          addedTo: "Fotos del verano",
        }),
      },
    },
    "Editor · GIF": { nav: editing("gif", "gifs"), editor: editorGif },
    "Editor · GIF creado": {
      nav: editing("gif", "gifs"),
      editor: {
        ...editorGif,
        creation: done({
          result: { meets_limit: true, size_bytes: 4_310_000, max_size_bytes: 10_000_000, fps: 15, duration_s: 1.9, reductions: [], notice: null },
          saved: "gato-saltando.gif",
        }),
      },
    },
    "GIFs · vacío": { nav: changeSection(initialNavigation(), "gifs") },
    "GIFs · creados": { nav: changeSection(initialNavigation(), "gifs"), gifs: createdGifs },
    "GIFs · error": {
      nav: changeSection(initialNavigation(), "gifs"),
      gifs: { error: "no se encontró `ffprobe`; instala FFmpeg (por ejemplo `sudo pacman -S ffmpeg`)" },
    },
    "Buscar GIF · tendencias": { nav: changeSection(initialNavigation(), "search-gif") },
    "Buscar GIF · GIPHY": { nav: changeSection(initialNavigation(), "search-gif"), searchGif: { mode: "con-clave", provider: "giphy" } },
    "Buscar GIF · sin clave": { nav: changeSection(initialNavigation(), "search-gif"), searchGif: { mode: "without-key" } },
    "Buscar GIF · clave no válida": { nav: changeSection(initialNavigation(), "search-gif"), searchGif: { mode: "key-invalid" } },
    "Buscar GIF · sin conexión": { nav: changeSection(initialNavigation(), "search-gif"), searchGif: { mode: "without-connection" } },
    "Buscar GIF · límite": { nav: changeSection(initialNavigation(), "search-gif"), searchGif: { mode: "limit" } },
    "Buscar GIF · sin resultados": { nav: changeSection(initialNavigation(), "search-gif"), searchGif: { mode: "sin-resultados" } },
    "Descargar · vacío": { nav: changeSection(initialNavigation(), "download") },
    "Descargar · lista": { nav: changeSection(initialNavigation(), "download"), download: { queue: completeQueue } },
    "Descargar · errores": {
      nav: changeSection(initialNavigation(), "download"),
      download: {
        linksError: "La línea 2 no es un enlace http o https: ftp://ejemplo.com/video",
        queueError: "Falta yt-dlp para descargar enlaces: no se encontró `yt-dlp`; instálalo (por ejemplo `sudo pacman -S yt-dlp`)",
        pasted: { kind: "notice", text: "Para archivos del equipo usa Imagen o Video." },
      },
    },
    "Aviso de pegado": {
      nav: changeSection(initialNavigation(), "gifs"),
      notice: "Una imagen fija no sirve para un GIF: la tienes en Descargar para hacer un sticker.",
    },
  };

  const register = (action: string) => (...data: unknown[]) => console.info(`demo: ${action}`, ...data);
  let liveNav: Navigation = initialNavigation();
  let notice: string | null = null;
  let noticeFolder: SavedFolder | null = null;
  let current: DemoState = {};
  const layers: (() => boolean)[] = [];
  const context: Context = {
    nav: () => liveNav,
    navigate(fresh) {
      liveNav = fresh;
      repaint();
    },
    notify(text, folder) {
      notice = text;
      noticeFolder = folder ?? null;
      repaint();
    },
    openFolder: register("abrir carpeta"),
    openWhatsapp: register("abrir WhatsApp"),
    entry: () => undefined,
    remember: () => undefined,
    forget: () => undefined,
    registerLayer: (close) => void layers.push(close),
    closeLayer: () => layers.some((close) => close()),
  };
  const shell = createShellView({
    goToRoot: (root) => context.navigate(goToRoot(liveNav, root)),
    goToCreate: () => context.navigate(goToRoot(liveNav, "create")),
    changeSection: (section) => context.navigate(changeSection(liveNav, section)),
    closeNotice: () => context.notify(null),
    openFolder: register("abrir carpeta"),
  });
  let catalog: CatalogSection | null = null;
  let klipy: KlipySection | null = null;
  let detail: DetailSection | null = null;
  let favorites: FavoritesSection | null = null;
  let searchGif: GifSearchSection | null = null;
  const stickers = createStickersView(
    {
      reload: register("reload"),
      pickFilter: register("filter"),
      openForm: register("abrir formulario"),
      closeForm: register("cerrar formulario"),
      sendForm: register("enviar formulario"),
      requestConfirmation: register("pedir confirmación"),
      cancelConfirmation: register("cancelar confirmación"),
      confirm: register("confirm"),
      openPack: register("abrir paquete"),
      closePack: register("cerrar paquete"),
      savePack: register("guardar paquete"),
      pickSticker: register("elegir sticker"),
      removeSticker: register("quitar sticker"),
      useAsIcon: register("usar como icono"),
      doExport: register("export"),
      addToWhatsapp: register("añadir a WhatsApp"),
      doImport: register("import"),
      createFromGallery: register("galería"),
      closeNotices: register("cerrar avisos"),
    },
    (path) => path,
  );
  const editor = createEditorView({
    back: register("back"),
    pickPreset: register("preset"),
    moveStart: register("start"),
    moveEnd: register("end"),
    moveRange: register("mover tramo"),
    pickDuration: register("duración"),
    autoFit: register("autoajuste"),
    releaseDrag: register("soltar tramo"),
    pickSide: register("tamaño GIF"),
    changeSpeed: register("speed"),
    fitMax: register("ajustar al máximo"),
    pickPriority: register("priority"),
    pickFps: register("fps"),
    setFraming: register("framing"),
    makeFit: register("makeFit"),
    pickMode: register("modo"),
    create: register("create"),
    cancelCreation: register("cancelar creación"),
    save: register("save"),
    saveSound: register("guardar sonido"),
    saveClipVideo: register("guardar vídeo para sticker"),
    openFolder: register("abrir carpeta"),
    openWhatsapp: register("abrir WhatsApp"),
    sendToExtension: register("enviar a extensión"),
    share: register("compartir"),
    sendToWhatsapp: register("enviar a WhatsApp"),
    addPack: register("añadir al paquete"),
  });
  const gifs = createGifsView({ pickVideo: register("elegir video"), save: register("guardar gif") });
  const download = createDownloadView({
    addLinks: async (text) => {
      console.info("demo: añadir enlaces", text);
      return false;
    },
    cancelDownloads: register("cancelar descargas"),
    resumeDownloads: register("descargar pendientes"),
    remove: register("remove"),
    makeSticker: register("hacer sticker"),
    makeGif: register("hacer gif"),
    saveVideo: register("guardar vídeo"),
    cancelSaving: register("cancelar guardado"),
    openFolder: register("abrir carpeta"),
    paste: register("paste"),
  });

  function repaint(): void {
    const state = current;
    stickers.render({ ...stickersBase, library: { kind: "ready", value: demo.library() }, ...state.stickers, openedPack: liveNav.openedPack });
    gifs.render({ choosing: false, error: null, created: [], ...state.gifs });
    download.render({ queue: emptyQueue(), sendingLinks: false, linksError: null, queueError: null, pasted: { kind: "inactive" }, saving: null, ...state.download });
    editor.render(liveNav.editor === null ? null : { ...editorVideo, ...state.editor });
    shell.render({ nav: liveNav, notice, noticeFolder });
    catalog?.onNavigate(liveNav);
    detail?.onNavigate(liveNav);
    favorites?.onNavigate(liveNav);
    searchGif?.onNavigate(liveNav);
  }

  function show(name: string): void {
    current = states[name] ?? {};
    liveNav = current.nav ?? initialNavigation();
    notice = current.notice ?? null;
    demo.withoutRed(current.red ?? "con-red");
    repaint();
    catalog?.reload(true);
    demoGifs.mode(current.searchGif?.mode ?? "con-clave", current.searchGif?.provider);
    klipy?.reload();
    if (liveNav.section === "search-gif") searchGif?.reload();
    const clicks = current.clicks ?? [];
    if (clicks.length > 0) {
      window.setTimeout(() => {
        for (const selector of clicks) document.querySelector<HTMLElement>(selector)?.click();
      }, 700);
    }
  }

  klipy = createKlipySection(context, {
    api: demoGifs.api,
    fileUrl: (path) => path,
    libraryState: () => ({ kind: "ready", value: demo.library() }),
    setLibrary: demo.setLibrary,
  });
  catalog = createCatalogSection(context, {
    klipy,
    api: demo.api,
    fileUrl: (path) => path,
    libraryState: () => ({ kind: "ready", value: demo.library() }),
    reloadLibrary: register("recargar biblioteca"),
    onLibraryChange: demo.onChange,
  });
  detail = createDetailSection(context, {
    api: demo.api,
    platform,
    fileUrl: (path) => path,
    libraryState: () => ({ kind: "ready", value: demo.library() }),
    setLibrary: demo.setLibrary,
    reloadLibrary: register("recargar biblioteca"),
    onLibraryChange: demo.onChange,
    catalog: catalog.catalog,
    reloadCatalog: catalog.reload,
  });
  favorites = createFavoritesSection(context, {
    api: demo.api,
    fileUrl: (path) => path,
    libraryState: () => ({ kind: "ready", value: demo.library() }),
    setLibrary: demo.setLibrary,
    onLibraryChange: demo.onChange,
    catalog: catalog.catalog,
  });
  searchGif = createGifSearchSection(context, { api: demoGifs.api, fileUrl: (path) => path });
  createBackupSection({ api: demo.api, setLibrary: demo.setLibrary });
  createExtensionsSection();
  catalog.onChange(detail.onCatalogChange);
  catalog.onChange(favorites.onCatalogChange);
  demo.onChange(() => repaint());

  const names = Object.keys(states);
  const selector = document.createElement("select");
  selector.setAttribute("aria-label", "Estado de ejemplo");
  selector.className = "demo-selector";
  for (const name of names) selector.append(new Option(name, name));
  selector.addEventListener("change", () => show(selector.value));
  document.body.append(selector);
  initial ??= new URLSearchParams(location.search).get("state");
  const chosen = initial === null ? names[0] : (names[Number(initial)] ?? (names.includes(initial) ? initial : names[0]));
  selector.value = chosen ?? "";
  show(selector.value);
}
