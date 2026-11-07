import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { dashOf, fillKind, stringOf, strokeKind, verbsOf } from "../rendering/GeometryBuffers";
import { Excalifont } from "../text/Excalifont";
import { drag, press, step, tap } from "../testing/Gestures";
import {
  type Core,
  type Editor,
  ExportScope,
  PointerKind,
  PointerPhase,
  Tool,
  loadCore,
} from "./Core";

const generated = new URL("../../generated/", import.meta.url);
const sample = readFileSync(new URL("sample.excalidraw", generated));

const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);

let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)), font);
});

const drawing = (elements: string) =>
  new TextEncoder().encode(`{"type":"excalidraw","version":2,"elements":[${elements}]}`);

describe("the core as wasm", () => {
  it("round-trips a Scene and keeps what it does not know", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"r","type":"rectangle","x":10,"y":20,"width":100,"height":50,"futureProperty":{"a":[1,2]}}`,
      ),
    );
    expect(editor.elementCount()).toBe(1);

    const saved = JSON.parse(new TextDecoder().decode(editor.save()));
    expect(saved.elements[0].futureProperty).toEqual({ a: [1, 2] });
    editor.free();
  });

  it("rejects a file that is not a Drawing", () => {
    const editor = core.makeEditor();
    expect(() => editor.load(new TextEncoder().encode("not json"))).toThrow();
    editor.free();
  });

  it("reads the geometry of a rectangle through views onto wasm memory", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"r","type":"rectangle","x":50,"y":100,"width":280,"height":180,"strokeColor":"#1e1e1e","strokeWidth":2,"backgroundColor":"#a5d8ff","fillStyle":"solid"}`,
      ),
    );
    const geometry = editor.geometry();
    geometry.withBuffers((buffers) => {
      expect(buffers.parts.length).toBe(2);
      const fill = buffers.parts.at(0);
      expect(fill.kind).toBe(fillKind);
      expect(fill.color).toBe(0xa5d8ffff);
      const stroke = buffers.parts.at(1);
      expect(stroke.kind).toBe(strokeKind);
      expect(stroke.color).toBe(0x1e1e1eff);
      expect(stroke.width).toBe(2);
      expect(verbsOf(buffers, stroke)[0]).toBe(0); // starts with a move
      expect(stroke.verbCount).toBeGreaterThan(0);
      expect(buffers.coords.length).toBeGreaterThan(0);
    });
    geometry.free();
    editor.free();
  });

  it("carries a dashed stroke's dash pattern", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"r","type":"rectangle","x":0,"y":0,"width":100,"height":50,"strokeStyle":"dashed"}`,
      ),
    );
    const geometry = editor.geometry();
    geometry.withBuffers((buffers) => {
      expect(dashOf(buffers.parts.at(0))).toEqual({ on: 8, off: 10 });
    });
    geometry.free();
    editor.free();
  });

  it("reads geometry correctly after memory grew", () => {
    const editor = core.makeEditor();
    editor.load(sample);
    const geometry = editor.geometry();
    const read = () => geometry.withBuffers((buffers) => buffers.parts.at(0).color);
    const before = read();
    // Growing memory detaches the buffer the views were made on.
    core.memory.grow(16);
    expect(read()).toBe(before);
    geometry.free();
    editor.free();
  });

  it("reports the Canvas colour and the viewport of the Drawing", () => {
    const editor = core.makeEditor();
    editor.load(sample);
    editor.setViewSize(800, 600);
    editor.setViewport({ originX: 0, originY: 0, zoom: 1 });
    editor.setTool(Tool.Selection);
    expect(editor.canvasColor()).toBe(0xffffffff);
    expect(editor.viewport().zoom).toBeGreaterThan(0);
    editor.setDarkMode(true);
    expect(editor.canvasColor()).not.toBe(0xffffffff);
    editor.free();
  });

  it("wraps a Label to its container with the font's measurements", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"box","type":"rectangle","x":0,"y":0,"width":120,"height":200,"roughness":0,"boundElements":[{"type":"text","id":"t"}]},` +
          `{"id":"t","type":"text","x":0,"y":0,"width":10,"height":25,"text":"A label that wraps","originalText":"A label that wraps","fontSize":20,"fontFamily":5,"lineHeight":1.25,"containerId":"box","textAlign":"center","verticalAlign":"middle"}`,
      ),
    );
    const geometry = editor.geometry();
    geometry.withBuffers((buffers) => {
      const lines = Array.from({ length: buffers.textLines.length }, (_, i) =>
        stringOf(buffers, buffers.textLines.at(i)),
      );
      expect(lines.join(" ")).toBe("A label that wraps");
      expect(lines.length).toBeGreaterThan(1);
      // The container is 120 wide with 5 of padding on each side.
      for (const line of lines) expect(font.lineWidth(line, 20)).toBeLessThanOrEqual(110);
    });
    geometry.free();
    editor.free();
  });
});

const noModifiers = { shift: false, ctrl: false, alt: false, meta: false };

const partCount = (editor: Editor, geometryOf: "overlay" | "geometry") => {
  const geometry = editor[geometryOf]();
  const count = geometry.withBuffers((buffers) => buffers.parts.length);
  geometry.free();
  return count;
};

describe("tools, overlay and undo through the wasm editor", () => {
  const box = drawing(
    `{"id":"r","type":"rectangle","x":100,"y":100,"width":200,"height":100,"roughness":0}`,
  );
  const selected = () => {
    const editor = core.makeEditor();
    editor.load(box);
    editor.setViewSize(600, 400);
    editor.setViewport({ originX: 0, originY: 0, zoom: 1 });
    editor.setTool(Tool.Selection);
    drag(editor, [[100, 100]]);
    return editor;
  };

  it("has an empty overlay until something is selected, then frames it with handles", () => {
    const editor = core.makeEditor();
    editor.load(box);
    editor.setViewSize(600, 400);
    expect(partCount(editor, "overlay")).toBe(0);
    const before = editor.overlayRevision();

    drag(editor, [[100, 100]]);
    // One frame, and a filled and framed square on each of the 8 handles
    // and the rotation handle.
    expect(partCount(editor, "overlay")).toBe(1 + 9 * 2);
    expect(editor.overlayRevision()).not.toBe(before);
    editor.free();
  });

  it("draws the Box-select rectangle while dragging on empty Canvas", () => {
    const editor = core.makeEditor();
    editor.load(box);
    editor.setViewSize(600, 400);
    press(editor, [
      [20, 20],
      [150, 150],
    ]);
    // Its fill and its frame, and the frame of the Element it already touches.
    expect(partCount(editor, "overlay")).toBeGreaterThanOrEqual(2);
    editor.free();
  });

  it("undoes and redoes a drawn shape and tells when it can", () => {
    const editor = core.makeEditor();
    editor.setViewSize(600, 400);
    expect(editor.canUndo()).toBe(false);
    editor.setTool(Tool.Rectangle);
    drag(editor, [
      [50, 50],
      [150, 120],
    ]);
    expect(editor.elementCount()).toBe(1);
    expect(editor.canUndo()).toBe(true);
    expect(editor.canRedo()).toBe(false);

    editor.undo();
    expect(editor.elementCount()).toBe(0);
    expect(editor.canRedo()).toBe(true);
    editor.redo();
    expect(editor.elementCount()).toBe(1);
    editor.free();
  });

  it("keeps the Tool after drawing while the Tool lock is on", () => {
    const editor = core.makeEditor();
    editor.setViewSize(600, 400);
    editor.setTool(Tool.Ellipse);
    drag(editor, [
      [50, 50],
      [150, 120],
    ]);
    expect(editor.tool()).toBe(Tool.Selection);

    editor.setToolLock(true);
    expect(editor.toolLock()).toBe(true);
    editor.setTool(Tool.Ellipse);
    drag(editor, [
      [200, 50],
      [300, 120],
    ]);
    expect(editor.tool()).toBe(Tool.Ellipse);
    expect(editor.elementCount()).toBe(2);

    editor.setToolLock(false);
    expect(editor.tool()).toBe(Tool.Selection);
    editor.free();
  });

  it("grows a Stroke while it is drawn", () => {
    const editor = core.makeEditor();
    editor.setViewSize(600, 400);
    editor.setTool(Tool.Freedraw);
    const down = (x: number, y: number, phase: PointerPhase) => editor.pointer(step(phase, [x, y]));
    down(10, 10, PointerPhase.Down);
    down(40, 30, PointerPhase.Move);
    const first = editor.revision();
    down(80, 20, PointerPhase.Move);
    expect(editor.elementCount()).toBe(1);
    expect(editor.revision()).not.toBe(first);
    down(80, 20, PointerPhase.Up);
    editor.free();
  });

  it("keeps a pen's pressure and simulates it for a mouse", () => {
    const pressuresSaved = (kind: PointerKind) => {
      const editor = core.makeEditor();
      editor.setViewSize(600, 400);
      editor.setTool(Tool.Freedraw);
      drag(
        editor,
        [
          [10, 10],
          [40, 30],
          [80, 20],
          [120, 60],
        ],
        kind,
        0.8,
      );
      const saved = JSON.parse(new TextDecoder().decode(editor.save()));
      editor.free();
      return saved.elements[0];
    };
    const pen = pressuresSaved(PointerKind.Pencil);
    expect(pen.simulatePressure).toBe(false);
    expect(pen.pressures.length).toBe(pen.points.length);
    expect(pen.pressures.every((p: number) => Math.abs(p - 0.8) < 1e-6)).toBe(true);

    const mouse = pressuresSaved(PointerKind.Mouse);
    expect(mouse.simulatePressure).toBe(true);
  });

  it("makes the overlay again for a zoom but not for a pan", () => {
    const editor = selected();
    const revision = editor.overlayRevision();
    editor.scroll({ x: 300, y: 200, dx: 30, dy: 10, modifiers: noModifiers });
    expect(editor.overlayRevision()).toBe(revision);
    editor.zoomIn();
    expect(editor.overlayRevision()).not.toBe(revision);
    editor.free();
  });

  describe("export", () => {
    const twoRectangles = () => {
      const editor = core.makeEditor();
      const rectangle = (id: string, x: number) =>
        `{"id":"${id}","type":"rectangle","x":${x},"y":100,"width":100,"height":50,"roundness":null}`;
      editor.load(drawing(`${rectangle("a", 100)},${rectangle("b", 300)}`));
      editor.setViewSize(800, 600);
      return editor;
    };

    it("frames the Scene with Excalidraw's padding of 10", () => {
      const editor = twoRectangles();
      const exported = editor.export(ExportScope.Scene)!;
      expect([exported.x, exported.y, exported.width, exported.height]).toEqual([90, 90, 320, 70]);
      exported.geometry.free();
      editor.free();
    });

    it("exports the Selection alone, and nothing if there is none", () => {
      const editor = twoRectangles();
      expect(editor.export(ExportScope.Selection)).toBeUndefined();

      drag(editor, [[300, 125]]);
      expect(editor.selection()).toEqual(["b"]);
      const exported = editor.export(ExportScope.Selection)!;
      expect([exported.x, exported.width]).toEqual([290, 120]);
      exported.geometry.free();
      editor.free();
    });

    it("has nothing to export from an empty Scene", () => {
      const editor = core.makeEditor();
      expect(editor.export(ExportScope.Scene)).toBeUndefined();
      editor.free();
    });
  });

  describe("Pencil-only drawing", () => {
    it("is off, turns on by itself with the first pen input and is then chosen", () => {
      const editor = core.makeEditor();
      expect(editor.pencilOnlyDrawing()).toBe(false);
      editor.setTool(Tool.Rectangle);
      drag(
        editor,
        [
          [10, 10],
          [60, 60],
        ],
        PointerKind.Pencil,
      );
      expect(editor.pencilOnlyDrawing()).toBe(true);
      editor.free();
    });

    it("lets fingers pan, not draw, while it is on", () => {
      const editor = core.makeEditor();
      editor.setPencilOnlyDrawing(true);
      editor.setTool(Tool.Rectangle);
      drag(
        editor,
        [
          [10, 10],
          [60, 60],
        ],
        PointerKind.Finger,
      );
      expect(editor.elementCount()).toBe(0);
      drag(
        editor,
        [
          [10, 10],
          [60, 60],
        ],
        PointerKind.Pencil,
      );
      expect(editor.elementCount()).toBe(1);
      editor.free();
    });

    it("stays off when the user chose so, whatever the pen does", () => {
      const editor = core.makeEditor();
      editor.setPencilOnlyDrawing(false);
      drag(
        editor,
        [
          [10, 10],
          [60, 60],
        ],
        PointerKind.Pencil,
      );
      expect(editor.pencilOnlyDrawing()).toBe(false);
      editor.free();
    });
  });
});

/** Creating and editing text through the editor: taps open the text editor where the core says, and what is typed reaches it. */
describe("editing text", () => {
  const hello = drawing(
    `{"id":"hello","type":"text","x":100,"y":50,"width":50,"height":25,"text":"Hello","originalText":"Hello","fontSize":20,"fontFamily":5,"lineHeight":1.25,"textAlign":"left","verticalAlign":"top","containerId":null,"autoResize":true,"version":3,"versionNonce":7}`,
  );
  const rectangle = drawing(
    `{"id":"r","type":"rectangle","x":100,"y":100,"width":200,"height":100,"strokeColor":"#1e1e1e","strokeWidth":2}`,
  );

  const open = (scene = hello) => {
    const editor = core.makeEditor();
    editor.load(scene);
    editor.setViewSize(800, 600);
    return editor;
  };

  it("opens the text editor at a tap with the text Tool, as the core says", () => {
    const editor = open();
    editor.setTool(Tool.Text);
    expect(editor.textEditor()).toBeUndefined();

    tap(editor, 300, 200);

    const shown = editor.textEditor()!;
    expect(shown.y).toBe(187.5);
    expect(shown.fontSize).toBe(20);
    expect(shown.text).toBe("");
    editor.free();
  });

  it("sizes the text editor to what is typed", () => {
    const editor = open();
    editor.setTool(Tool.Text);
    tap(editor, 300, 200);

    editor.setEditingText("ll");

    // Excalifont's "l" advances 225 units of 1000: two at 20 points.
    const shown = editor.textEditor()!;
    expect(shown.text).toBe("ll");
    expect(shown.width).toBeCloseTo(9, 9);
    editor.free();
  });

  it("follows zooming with the font size and position", () => {
    const editor = open();
    editor.setTool(Tool.Text);
    tap(editor, 300, 200);
    const before = editor.textEditor()!;

    editor.zoomIn();

    const shown = editor.textEditor()!;
    expect(shown.zoom).toBeCloseTo(1.1, 9);
    expect(shown.fontSize).toBeCloseTo(22, 9);
    expect(shown.x).not.toBe(before.x);
    editor.free();
  });

  it("keeps the text and ends the editing on commit, as one undo step", () => {
    const editor = open();
    editor.setTool(Tool.Text);
    tap(editor, 300, 200);
    editor.setEditingText("Hi");

    editor.commitText();

    expect(editor.textEditor()).toBeUndefined();
    expect(editor.elementCount()).toBe(2);
    expect(editor.canUndo()).toBe(true);
    editor.free();
  });

  it("does not keep empty text", () => {
    const editor = open();
    editor.setTool(Tool.Text);
    tap(editor, 300, 200);

    editor.commitText();

    expect(editor.elementCount()).toBe(1);
    expect(editor.canUndo()).toBe(false);
    editor.free();
  });

  it("commits on a tap elsewhere on the Canvas", () => {
    const editor = open();
    editor.setTool(Tool.Text);
    tap(editor, 300, 200);
    editor.setEditingText("Hi");

    tap(editor, 600, 500);

    expect(editor.textEditor()).toBeUndefined();
    expect(editor.elementCount()).toBe(2);
    editor.free();
  });

  it("opens a double-tap on a text with its text", () => {
    const editor = open();

    tap(editor, 120, 60);
    expect(editor.textEditor()).toBeUndefined();
    tap(editor, 120, 60);

    expect(editor.textEditor()?.text).toBe("Hello");
    editor.free();
  });

  it("opens the Label of a double-tapped shape and of a selected shape on Enter", () => {
    const editor = open(rectangle);
    tap(editor, 200, 150);
    tap(editor, 200, 150);
    expect(editor.textEditor()).toBeDefined();
    editor.cancelText();
    expect(editor.textEditor()).toBeUndefined();

    editor.setTool(Tool.Selection);
    // An empty shape is hit on its edge.
    tap(editor, 100, 150);
    editor.editSelectedText();
    expect(editor.textEditor()).toBeDefined();
    editor.free();
  });
});
