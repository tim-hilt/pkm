/**
 * The GPU device the renderer needs (ADR-0015), or `undefined` where the
 * browser has no WebGPU or no adapter: there is no fallback renderer.
 */
export async function acquireDevice(gpu: GPU | undefined): Promise<GPUDevice | undefined> {
  if (!gpu) return undefined;
  try {
    const adapter = await gpu.requestAdapter();
    return adapter ? await adapter.requestDevice() : undefined;
  } catch {
    return undefined;
  }
}

/** Replaces the page with a note that the browser is not supported. */
export function showUnsupportedPage(root: HTMLElement): void {
  root.replaceChildren();
  const page = document.createElement("main");
  page.className = "unsupported";
  const heading = document.createElement("h1");
  heading.textContent = "This browser is not supported";
  const text = document.createElement("p");
  text.textContent =
    "Excalidraw Native draws with WebGPU. Please use a recent Chrome, Edge, Safari (26 or newer) or Firefox (141 or newer, on Windows and macOS).";
  page.append(heading, text);
  root.append(page);
}
