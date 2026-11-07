import type { Geometry, Viewport } from "../core/Core";
import type { GeometrySource } from "./GeometryBuffers";

/** What the editor offers a Canvas that shows it; `Editor` satisfies it. */
export interface SyncedEditor {
  revision(): number;
  viewport(): Viewport;
  canvasColor(): number;
  geometry(): Geometry;
  overlayRevision(): number;
  overlay(): Geometry;
}

/** What shows the Scene; `CanvasRenderer` satisfies it. */
export interface SceneTarget {
  setGeometry(geometry: GeometrySource): void;
  /** What is drawn over the Scene. */
  setOverlay(overlay: GeometrySource): void;
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
  private shownOverlayRevision: number | undefined;
  private shownOverlay: Geometry | undefined;

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
    this.syncOverlay();
    this.syncViewport();
  }

  /** Call after input or a command reached the editor. Tells whether the Scene changed. */
  sync(): boolean {
    const changed = this.editor.revision() !== this.shownRevision;
    if (changed) this.showScene();
    else {
      this.syncOverlay();
      this.syncViewport();
    }
    return changed;
  }

  /** The overlay is made for a zoom and depends on the Selection: it has its own revision. */
  private syncOverlay(): void {
    const { editor, target } = this;
    if (editor.overlayRevision() === this.shownOverlayRevision) return;
    this.shownOverlayRevision = editor.overlayRevision();
    const old = this.shownOverlay;
    this.shownOverlay = editor.overlay();
    target.setOverlay(this.shownOverlay);
    old?.free();
  }

  /** Frees what is shown; call once the target no longer draws it. */
  dispose(): void {
    this.shown?.free();
    this.shownOverlay?.free();
    this.shown = this.shownOverlay = undefined;
  }

  private syncViewport(): void {
    this.target.viewport = this.editor.viewport();
  }
}
