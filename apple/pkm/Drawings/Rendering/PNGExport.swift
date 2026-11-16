import CoreTransferable
import Foundation
import Metal
import UniformTypeIdentifiers

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// Exports the Scene or the Selection as a PNG at 2x, drawn by the Canvas's
/// own Metal renderer offscreen (ADR-0002) from the core's export geometry
/// and frame (ADR-0008), on white paper.
@MainActor
enum PNGExport {
    /// Device pixels per Scene unit, like Excalidraw's 2x export.
    static let scale: Float = 2

    /// Nil if there is nothing to export or the GPU failed.
    static func render(_ editor: Editor, scope: ExportScope) -> RenderedImage? {
        guard let export = editor.export(scope: scope),
            let device = MTLCreateSystemDefaultDevice(),
            let renderer = CanvasRenderer(device: device)
        else { return nil }
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
}

/// What Export shows and hands on, as excalidraw.com's Export image dialog:
/// the PNG of the Selection, or of the Scene with nothing selected, made once
/// when Export opens.
struct ExportedImage: Identifiable, Transferable {
    let id = UUID()
    let name: String
    let image: RenderedImage
    let png: Data

    /// Nil if there is nothing to export.
    @MainActor init?(_ editor: Editor, name: String) {
        let scope: ExportScope = editor.selection().isEmpty ? .scene : .selection
        guard let image = PNGExport.render(editor, scope: scope) else { return nil }
        self.name = name
        self.image = image
        png = image.pngData()
    }

    static var transferRepresentation: some TransferRepresentation {
        DataRepresentation(exportedContentType: .png) { $0.png }
            .suggestedFileName { "\($0.name).png" }
    }

    @MainActor func copyToPasteboard() {
        #if os(macOS)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setData(png, forType: .png)
        #else
            UIPasteboard.general.setData(png, forPasteboardType: UTType.png.identifier)
        #endif
    }
}
