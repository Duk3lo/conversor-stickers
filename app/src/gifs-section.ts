import type { Context } from "./context.ts";
import * as commands from "./contract.ts";
import { asErrorUi } from "./contract.ts";
import { errorMessage } from "./messages.ts";
import { openEditor } from "./navigation.ts";
import type { ReadyGif } from "./editor-section.ts";
import { DEFAULT_IMAGE_SECONDS, MAX_GIF_IMAGES, clampImageSeconds, imagesGifRequest } from "./images-gif.ts";
import { createGifsView } from "./gifs-view.ts";
import type { GifsView } from "./views.ts";

export function createGifsSection(context: Context): { addCreated(gif: ReadyGif): void } {
  let view: GifsView = { choosing: false, choosingImages: false, images: null, error: null, created: [] };

  function set(changes: Partial<GifsView>): void {
    view = { ...view, ...changes };
    screen.render(view);
  }

  function changeGif(entry: number, changes: Partial<GifsView["created"][number]>): void {
    set({ created: view.created.map((gif) => (gif.entry === entry ? { ...gif, ...changes } : gif)) });
  }

  async function pickVideo(): Promise<void> {
    set({ choosing: true, error: null });
    try {
      const entries = await commands.pickFromGallery("video");
      entries.forEach((entry) => context.remember(entry));
      const first = entries[0];
      if (first === undefined) return set({ choosing: false });
      if (first.media?.kind !== "video") {
        return set({ choosing: false, error: `${first.name} no se pudo abrir como video.` });
      }
      set({ choosing: false });
      context.navigate(openEditor(context.nav(), first.id, "gif"));
    } catch (failure) {
      set({ choosing: false, error: errorMessage(asErrorUi(failure)) });
    }
  }

  async function pickImages(): Promise<void> {
    set({ choosingImages: true, error: null });
    try {
      const entries = await commands.pickFromGallery("image");
      entries.forEach((entry) => context.remember(entry));
      if (entries.length === 0) return set({ choosingImages: false });
      if (entries.length > MAX_GIF_IMAGES) {
        return set({ choosingImages: false, error: `Elige como mucho ${MAX_GIF_IMAGES} imágenes.` });
      }
      const unreadable = entries.find((entry) => entry.media?.kind !== "image");
      if (unreadable !== undefined) {
        return set({ choosingImages: false, error: `${unreadable.name} no se pudo abrir como imagen.` });
      }
      set({
        choosingImages: false,
        images: { ids: entries.map((entry) => entry.id), imageSeconds: DEFAULT_IMAGE_SECONDS, repeat: true, creating: false },
      });
    } catch (failure) {
      set({ choosingImages: false, error: errorMessage(asErrorUi(failure)) });
    }
  }

  async function createFromImages(): Promise<void> {
    const images = view.images;
    if (images === null || images.creating) return;
    const first = images.ids[0];
    if (first === undefined) return;
    set({ images: { ...images, creating: true }, error: null });
    try {
      const sizeBytes = await commands.createImagesGif(imagesGifRequest(images.ids, images.imageSeconds, images.repeat));
      const bytes = await commands.result(first);
      set({ images: null });
      addCreated({
        entry: first,
        name: context.entry(first)?.name ?? "imagenes",
        destination: "GIF",
        sizeBytes,
        bytes,
        kind: "image/gif",
      });
    } catch (failure) {
      const now = view.images;
      set({ images: now === null ? null : { ...now, creating: false }, error: errorMessage(asErrorUi(failure)) });
    }
  }

  async function save(entry: number): Promise<void> {
    const gif = view.created.find((candidate) => candidate.entry === entry);
    if (gif === undefined || gif.saving) return;
    changeGif(entry, { saving: true, saveError: null });
    try {
      const name = await commands.save(entry);
      changeGif(entry, { saving: false, saved: name ?? gif.saved });
    } catch (failure) {
      changeGif(entry, { saving: false, saveError: errorMessage(asErrorUi(failure)) });
    }
  }

  const screen = createGifsView({
    pickVideo: () => void pickVideo(),
    pickImages: () => void pickImages(),
    setImageSeconds: (seconds) => {
      if (view.images !== null) set({ images: { ...view.images, imageSeconds: clampImageSeconds(seconds) } });
    },
    setRepeat: (repeat) => {
      if (view.images !== null) set({ images: { ...view.images, repeat } });
    },
    createFromImages: () => void createFromImages(),
    discardImages: () => set({ images: null, error: null }),
    save: (entry) => void save(entry),
  });
  screen.render(view);

  function addCreated(gif: ReadyGif): void {
    const fresh = {
      entry: gif.entry,
      name: gif.name,
      destination: gif.destination,
      sizeBytes: gif.sizeBytes,
      image: URL.createObjectURL(new Blob([gif.bytes], { type: gif.kind })),
      saving: false,
      saved: null,
      saveError: null,
    };
    const previous = view.created.find((candidate) => candidate.entry === gif.entry);
    if (previous !== undefined) URL.revokeObjectURL(previous.image);
    set({ created: [fresh, ...view.created.filter((candidate) => candidate.entry !== gif.entry)] });
  }

  return { addCreated };
}
