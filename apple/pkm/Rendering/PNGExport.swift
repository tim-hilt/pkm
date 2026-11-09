import CoreTransferable
import Foundation
import Metal
import UniformTypeIdentifiers

/// Exports the Scene or the Selection as a PNG at 2x, drawn by the Canvas's
/// own Metal renderer offscreen (ADR-0002) from the core's export geometry
/// and frame (ADR-0008), on white paper or on nothing.
@MainActor
enum PNGExport {
    /// Device pixels per Scene unit, like Excalidraw's 2x export.
    static let scale: Float = 2

    /// Nil if there is nothing to export or the GPU failed.
    static func render(_ editor: Editor, scope: ExportScope, background: Bool) -> RenderedImage? {
        guard let export = editor.export(scope: scope),
            let device = MTLCreateSystemDefaultDevice(),
            let renderer = CanvasRenderer(device: device)
        else { return nil }
        renderer.drawsPaper = background
        renderer.files = editor
        renderer.setGeometry(export.geometry)
        renderer.viewport = Viewport(origin: SIMD2(Float(export.x), Float(export.y)), zoom: 1)
        return renderer.renderOffscreen(
            width: pixels(export.width), height: pixels(export.height), pixelsPerPoint: scale)
    }

    /// Rounded, not truncated or rounded up, as Excalidraw sizes its export
    /// canvas (`exportToCanvas`), so that both export at the same size.
    private static func pixels(_ length: Double) -> Int {
        max(1, Int((length * Double(scale)).rounded()))
    }

    /// The export as PNG file content; nil as for `render`.
    static func pngData(_ editor: Editor, scope: ExportScope, background: Bool) -> Data? {
        render(editor, scope: scope, background: background)?.pngData()
    }
}

/// A PNG export handed to the share sheet; drawn only when something is shared.
struct ExportedPNG: Transferable {
    let name: String
    let editor: Editor
    let scope: ExportScope

    static var transferRepresentation: some TransferRepresentation {
        DataRepresentation(exportedContentType: .png) { png in
            let data = await MainActor.run {
                PNGExport.pngData(png.editor, scope: png.scope, background: true)
            }
            guard let data else { throw CocoaError(.fileWriteUnknown) }
            return data
        }
        .suggestedFileName { "\($0.name).png" }
    }
}
