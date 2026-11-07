import type { Geometry, Viewport } from "../core/Core";
import type { GeometrySource } from "./GeometryBuffers";

/** What the editor offers a Canvas that shows it; `Editor` satisfies it. */
export interface SyncedEditor {
  revision(): number;
  viewport(): Viewport;
  canvasColor(): number;
  geometry(): Geometry;
}

/** What shows the Scene; `CanvasRenderer` satisfies it. */
export interface SceneTarget {
  setGeometry(geometry: GeometrySource): void;
  viewport: Viewport;
  paperColor: number;
}

/**
 * Keeps a renderer showing an editor's Scene (the Swift Canvas's `syncScene`
 * and `syncViewport`). Panning and zooming only hand the renderer a new
 * viewport; the geometry is asked for again only when the editor's revision
 * moved (ADR-0007).
 */
export class SceneSync {
  private shownRevision: number | undefined;
  private shown: Geometry | undefined;

  constructor(
    private readonly editor: SyncedEditor,
    private readonly target: SceneTarget,
  ) {}

  /** Shows the Scene as it is now, whatever was shown before. */
  showScene(): void {
    const { editor, target } = this;
    this.shownRevision = editor.revision();
    const old = this.shown;
    this.shown = editor.geometry();
    target.setGeometry(this.shown);
    target.paperColor = editor.canvasColor();
    old?.free();
    this.syncViewport();
  }

  /** Call after input or a command reached the editor. Tells whether the Scene changed. */
  sync(): boolean {
    const changed = this.editor.revision() !== this.shownRevision;
    if (changed) this.showScene();
    else this.syncViewport();
    return changed;
  }

  private syncViewport(): void {
    this.target.viewport = this.editor.viewport();
  }
}
