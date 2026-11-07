import { webFontFamily } from "./TextViewStyle";

/**
 * Makes the Excalifont `bytes` available to the page's CSS as "Excalifont", for
 * the Text view's `<textarea>` (the Canvas draws text from outlines instead).
 * Loaded from the bytes the app already has, so that it works offline too.
 */
export async function loadExcalifontWebFont(bytes: Uint8Array): Promise<void> {
  const face = new FontFace(webFontFamily, bytes.slice().buffer);
  document.fonts.add(await face.load());
}
