import { afterEach, beforeAll, describe, it } from "vitest";
import type { Editor } from "../../core/Core";
import { expectMatchesReference } from "../testing/BrowserReferences";
import { drag, press } from "../testing/Gestures";
import { type RenderHarness, makeRenderHarness } from "../testing/OffscreenRender";

/**
 * The lines of Dimensions over the overlay while a box is resized, and the
 * same fading out. The numbers are the labels' (DOM), not drawn here.
 */

let harness: RenderHarness;
beforeAll(async () => {
  harness = await makeRenderHarness();
});

const editors: Editor[] = [];
afterEach(() => {
  editors.splice(0).forEach((editor) => editor.free());
});

/** A box being resized by its south-east handle (4 beyond its corner), with Dimensions on. */
function resizing(): Editor {
  const editor = harness.core.makeEditor();
  editors.push(editor);
  editor.load(
    new TextEncoder().encode(
      `{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":100,"y":100,"width":120,"height":80,"roughness":0}]}`,
    ),
  );
  editor.setViewSize(400, 400);
  editor.setViewport({ originX: 0, originY: 0, zoom: 1 });
  editor.setShowDimensions(true);
  drag(editor, [[100, 140]]);
  press(editor, [
    [224, 184],
    [266, 216],
  ]);
  return editor;
}

async function expectDimensions(name: string, strength: number) {
  const editor = resizing();
  const scene = editor.geometry();
  const overlay = editor.overlay();
  const dimensions = editor.dimensions()!;
  const image = await harness.render(scene, editor.viewport(), {
    overlay,
    dimensions: { lines: dimensions.geometry, strength },
  });
  scene.free();
  overlay.free();
  dimensions.geometry.free();
  await expectMatchesReference(image, name);
}

describe("Dimensions reference images", () => {
  it("draws the lines and ticks beside a box being resized", async () => {
    await expectDimensions("dimensions-resize", 1);
  });

  it("draws them fainter while they fade out", async () => {
    await expectDimensions("dimensions-fading", 0.5);
  });
});
