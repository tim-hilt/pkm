import { afterEach, beforeAll, describe, it } from "vitest";
import { type Editor, Tool } from "../core/Core";
import { expectMatchesReference } from "../testing/BrowserReferences";
import { drag, press } from "../testing/Gestures";
import { type RenderHarness, makeRenderHarness } from "../testing/OffscreenRender";

/**
 * What is drawn over the Scene to show the state of editing, made by driving
 * the core's editor with pointer events and compared with stored reference
 * images (docs/web-renderer-reference-images.md). Swift has no references for
 * these; the shapes follow the core's overlay description.
 */

let harness: RenderHarness;
beforeAll(async () => {
  harness = await makeRenderHarness();
});

const editors: Editor[] = [];
afterEach(() => {
  editors.splice(0).forEach((editor) => editor.free());
});

const drawing = (...elements: string[]) =>
  new TextEncoder().encode(`{"type":"excalidraw","version":2,"elements":[${elements.join(",")}]}`);

const box = (id: string, x: number, y: number, extra = "") =>
  `{"id":"${id}","type":"rectangle","x":${x},"y":${y},"width":120,"height":80,"roughness":0${extra}}`;

/** An editor showing `elements` in a 400 x 400 view at zoom 1. */
function editorWith(...elements: string[]): Editor {
  const editor = harness.core.makeEditor();
  editors.push(editor);
  editor.load(drawing(...elements));
  editor.setViewSize(400, 400);
  editor.setViewport({ originX: 0, originY: 0, zoom: 1 });
  return editor;
}

async function expectOverlay(editor: Editor, name: string) {
  const scene = editor.geometry();
  const overlay = editor.overlay();
  const image = await harness.render(scene, editor.viewport(), { overlay });
  scene.free();
  overlay.free();
  await expectMatchesReference(image, name);
}

describe("Overlay reference images", () => {
  it("frames a selected Element and shows its transform handles", async () => {
    const editor = editorWith(box("a", 100, 100));
    drag(editor, [[100, 100]]);
    await expectOverlay(editor, "overlay-selection");
  });

  it("frames a turned Element along with it", async () => {
    const editor = editorWith(
      box("a", 100, 100, `,"angle":0.5,"backgroundColor":"#a5d8ff","fillStyle":"solid"`),
    );
    drag(editor, [[160, 140]]);
    await expectOverlay(editor, "overlay-selection-turned");
  });

  it("frames a group with a dashed frame", async () => {
    const editor = editorWith(
      box("a", 40, 60, `,"groupIds":["g"]`),
      box("b", 200, 200, `,"groupIds":["g"]`),
    );
    drag(editor, [[40, 60]]);
    await expectOverlay(editor, "overlay-group");
  });

  it("shows the handles on the points of a selected line", async () => {
    const editor = editorWith(
      `{"id":"l","type":"line","x":60,"y":200,"width":240,"height":-100,"roughness":0,"points":[[0,0],[120,-100],[240,-20]]}`,
    );
    drag(editor, [[120, 150]]);
    await expectOverlay(editor, "overlay-point-handles");
  });

  it("draws the Box-select rectangle while dragging", async () => {
    const editor = editorWith(box("a", 100, 100));
    press(editor, [
      [30, 40],
      [250, 260],
    ]);
    await expectOverlay(editor, "overlay-box-select");
  });

  it("highlights the shape an arrow end would bind to", async () => {
    const editor = editorWith(box("a", 200, 140));
    editor.setTool(Tool.Arrow);
    press(editor, [
      [40, 180],
      [230, 180],
    ]);
    await expectOverlay(editor, "overlay-binding-highlight");
  });

  it("draws the overlay at a zoom with as thin a frame", async () => {
    const editor = editorWith(box("a", 100, 100));
    editor.setViewport({ originX: 50, originY: 50, zoom: 2 });
    drag(editor, [[100, 180]]);
    await expectOverlay(editor, "overlay-selection-zoomed");
  });
});
