/** Where the images that reach the Canvas from outside go. */
export interface ImageSink {
  /** The bytes of image files, and where on the Canvas (in its coordinates) they go; none for the middle of the view. */
  images(sources: Uint8Array[], at?: { x: number; y: number }): void;
}

const isImage = (file: File) => file.type.startsWith("image/");

const bytesOf = async (files: File[]) =>
  Promise.all(files.map(async (file) => new Uint8Array(await file.arrayBuffer())));

/** Asks the user for image files with the browser's file picker; none if they cancel. */
export function pickImageFiles(): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.multiple = true;
    input.addEventListener("change", () => resolve([...(input.files ?? [])]), { once: true });
    input.addEventListener("cancel", () => resolve([]), { once: true });
    input.click();
  });
}

/** Opens the file picker and hands what was chosen to `sink`. */
export async function pickImages(sink: ImageSink): Promise<void> {
  const sources = await bytesOf(await pickImageFiles());
  if (sources.length > 0) sink.images(sources);
}

/**
 * Makes the Canvas take images dropped on it, where they were dropped, as the
 * Swift `CanvasImageInput` does. Pasted images arrive through
 * `CanvasClipboard`, with the Elements a paste may hold instead.
 */
export class ImageInput {
  private readonly listeners = new AbortController();

  constructor(
    private readonly canvas: HTMLElement,
    private readonly sink: ImageSink,
  ) {
    const { signal } = this.listeners;
    canvas.addEventListener(
      "dragover",
      (e) => {
        if (hasImages(e.dataTransfer)) {
          e.preventDefault();
          e.dataTransfer!.dropEffect = "copy";
        }
      },
      { signal },
    );
    canvas.addEventListener(
      "drop",
      (e) => {
        const files = imagesOf(e.dataTransfer);
        if (files.length === 0) return;
        e.preventDefault();
        const bounds = canvas.getBoundingClientRect();
        const at = { x: e.clientX - bounds.left, y: e.clientY - bounds.top };
        void bytesOf(files).then((sources) => sink.images(sources, at));
      },
      { signal },
    );
  }

  /** Stops listening, for when the Canvas goes away. */
  dispose(): void {
    this.listeners.abort();
  }
}

/** Whether a drag carries files that may be images (the type of the files is only known on the drop). */
function hasImages(data: DataTransfer | null): boolean {
  if (!data) return false;
  return [...data.items].some(
    (item) => item.kind === "file" && (!item.type || item.type.startsWith("image/")),
  );
}

function imagesOf(data: DataTransfer | null): File[] {
  return [...(data?.files ?? [])].filter(isImage);
}
