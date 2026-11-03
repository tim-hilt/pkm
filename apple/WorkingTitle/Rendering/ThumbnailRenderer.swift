import Metal

/// A Drawing prepared for a thumbnail: its Scene framed for the thumbnail.
/// Everything that needs no GPU happens here, so that it can run off the main
/// thread (loading, laying out text, building the core's geometry).
struct ThumbnailScene: Sendable {
    /// Keeps the image files of the Scene for the renderer.
    let editor: Editor
    let geometry: Geometry
    let viewport: Viewport

    /// Nil if `drawing` (a `.excalidraw` file's bytes) is not a valid Drawing.
    init?(drawing: Data) {
        let editor = Editor.withExcalifont()
        guard (try? editor.load(bytes: drawing)) != nil else { return nil }
        // The core frames the Scene in a view as big as the thumbnail less its
        // margin; the margin is put back by moving the viewport.
        let margin = ThumbnailRenderer.margin
        editor.setViewSize(
            width: Double(ThumbnailRenderer.width) - 2 * margin, height: Double(ThumbnailRenderer.height) - 2 * margin)
        editor.zoomToFit()
        var viewport = Viewport(editor.viewport())
        viewport.origin -= SIMD2(repeating: Float(margin) / viewport.zoom)
        self.editor = editor
        self.geometry = editor.geometry()
        self.viewport = viewport
    }
}

/// Draws thumbnails of Drawings for the Gallery with the Canvas's own Metal
/// renderer, offscreen (ADR-0002): the whole Scene, framed like "zoom to fit"
/// (never enlarged past 100 %), on the Canvas's white paper.
///
/// One renderer is kept for all thumbnails, as making it is costly; it holds
/// on to the last Drawing's draws until the next one replaces them.
@MainActor
final class ThumbnailRenderer {
    /// Pixels. 4:3, like a Gallery card, and enough for a card of 260 points on
    /// a 2x screen; kept small because many thumbnails are held in memory.
    nonisolated static let width = 512
    nonisolated static let height = 384
    /// Paper kept free around the Scene, in pixels.
    nonisolated static let margin = 16.0

    private let renderer: CanvasRenderer

    /// Thumbnails drawn so far; tests use it to see what was drawn again.
    private(set) var rendered = 0

    /// Nil if there is no GPU or the renderer cannot be made.
    init?() {
        guard let device = MTLCreateSystemDefaultDevice(), let renderer = CanvasRenderer(device: device)
        else { return nil }
        self.renderer = renderer
    }

    func render(_ scene: ThumbnailScene) -> RenderedImage? {
        // Images are looked up by file id, which different Drawings may use
        // for different files.
        renderer.textures.keepOnly([])
        renderer.files = scene.editor
        renderer.setGeometry(scene.geometry)
        renderer.viewport = scene.viewport
        let image = renderer.renderOffscreen(width: Self.width, height: Self.height)
        if image != nil { rendered += 1 }
        return image
    }

    /// Nil if `drawing` is not a valid Drawing or the GPU failed.
    func render(drawing: Data) -> RenderedImage? {
        ThumbnailScene(drawing: drawing).flatMap(render)
    }
}
