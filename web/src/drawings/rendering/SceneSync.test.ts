import { describe, expect, it } from "vitest";
import type { Geometry, Viewport } from "../../core/Core";
import { SceneSync, type SceneTarget, type SyncedEditor } from "./SceneSync";

class FakeEditor implements SyncedEditor {
  revisionValue = 1;
  viewportValue: Viewport = { originX: 0, originY: 0, zoom: 1 };
  overlayRevisionValue = 1;
  geometryCalls = 0;
  overlayCalls = 0;
  freed = 0;
  overlayRevision = () => this.overlayRevisionValue;
  overlay() {
    this.overlayCalls++;
    return { free: () => this.freed++ } as unknown as Geometry;
  }
  revision = () => this.revisionValue;
  viewport = () => this.viewportValue;
  canvasColor = () => 0xffffffff;
  geometry() {
    this.geometryCalls++;
    return { free: () => this.freed++ } as unknown as Geometry;
  }
}

const target = (): SceneTarget & { geometries: unknown[]; overlays: unknown[] } => {
  const t = {
    geometries: [] as unknown[],
    overlays: [] as unknown[],
    setOverlay(o: unknown) {
      t.overlays.push(o);
    },
    viewport: { originX: 0, originY: 0, zoom: 1 },
    paperColor: 0,
    setGeometry(g: unknown) {
      t.geometries.push(g);
    },
  };
  return t;
};

describe("SceneSync", () => {
  it("asks for geometry again only when the revision moved", () => {
    const editor = new FakeEditor();
    const view = target();
    const sync = new SceneSync(editor, view);
    sync.showScene();
    expect(editor.geometryCalls).toBe(1);

    editor.viewportValue = { originX: 5, originY: 0, zoom: 2 };
    expect(sync.sync()).toBe(false);
    expect(editor.geometryCalls).toBe(1);
    expect(view.viewport).toEqual({ originX: 5, originY: 0, zoom: 2 });

    editor.revisionValue = 2;
    expect(sync.sync()).toBe(true);
    expect(editor.geometryCalls).toBe(2);
    expect(view.geometries.length).toBe(2);
  });

  it("frees the geometry it replaced", () => {
    const editor = new FakeEditor();
    const sync = new SceneSync(editor, target());
    sync.showScene();
    editor.revisionValue = 2;
    sync.sync();
    expect(editor.freed).toBe(1);
  });

  it("asks for the overlay again only when its revision moved, not for the Scene's", () => {
    const editor = new FakeEditor();
    const view = target();
    const sync = new SceneSync(editor, view);
    sync.showScene();
    expect(editor.overlayCalls).toBe(1);

    editor.revisionValue = 2;
    sync.sync();
    expect(editor.overlayCalls).toBe(1);

    editor.overlayRevisionValue = 2;
    sync.sync();
    expect(editor.overlayCalls).toBe(2);
    expect(view.overlays.length).toBe(2);
    // The overlay it replaced is freed (the Scene's geometry was freed once before).
    expect(editor.freed).toBe(2);
  });
});
