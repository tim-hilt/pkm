import Metal

@testable import WorkingTitle

/// Renders geometry the way the Canvas does, into memory instead of a view.
@MainActor
enum OffscreenRender {
    struct Unavailable: Error {}

    /// A production renderer on the system's GPU.
    static func makeRenderer() throws -> CanvasRenderer {
        guard let device = MTLCreateSystemDefaultDevice(), let renderer = CanvasRenderer(device: device)
        else { throw Unavailable() }
        return renderer
    }

    /// Draws `geometry` seen through `viewport` into a `size` x `size` pixel
    /// image, with the production renderer and its 4x MSAA. `pixelsPerPoint`
    /// is the screen scale being simulated. `files` supplies the bytes of images.
    ///
    /// - Returns: the finished image and the renderer that drew it, so tests
    ///   can inspect renderer state such as the frame counter.
    static func render(
        _ geometry: any GeometrySource, viewport: Viewport, pixelsPerPoint: Float = 1, size: Int = 400,
        files: (any ImageFileSource)? = nil
    ) throws -> (image: RenderedImage, renderer: CanvasRenderer) {
        let renderer = try makeRenderer()
        renderer.files = files
        renderer.setGeometry(geometry)
        renderer.viewport = viewport
        return (draw(with: renderer, pixelsPerPoint: pixelsPerPoint, size: size), renderer)
    }

    /// Renders one frame of whatever `renderer` has been given.
    static func draw(with renderer: CanvasRenderer, pixelsPerPoint: Float = 1, size: Int = 400)
        -> RenderedImage
    {
        renderer.renderOffscreen(width: size, height: size, pixelsPerPoint: pixelsPerPoint)!
    }
}
