import SwiftUI

/// Where the Export PNG options are remembered: one setting for the main menu
/// on iPhone and iPad and the menu bar on a Mac.
enum ExportPreferences {
    static let backgroundKey = "exportPNGBackground"
}

#if os(macOS)
    import AppKit
    import UniformTypeIdentifiers

    /// The Drawing in front as the menu bar's File > Export items act on it;
    /// each asks where to save, with the Drawing's name filled in. (iPhone and
    /// iPad share instead, from the main menu.)
    struct DrawingExporter {
        let drawing: OpenDrawing
        let export: ExportedDrawing

        /// The `.excalidraw` file, exactly as stored.
        @MainActor func exportDrawing() {
            guard let file = export.gallery.exportFile(named: drawing.name),
                let data = try? Data(contentsOf: file)
            else { return }
            save(data, as: "\(drawing.name).excalidraw", type: .excalidraw)
        }

        @MainActor func exportPNG(scope: ExportScope, background: Bool) {
            guard let data = PNGExport.pngData(drawing.editor, scope: scope, background: background) else { return }
            save(data, as: "\(drawing.name).png", type: .png)
        }

        @MainActor private func save(_ data: Data, as name: String, type: UTType) {
            let panel = NSSavePanel()
            panel.nameFieldStringValue = name
            panel.allowedContentTypes = [type]
            panel.begin { response in
                guard response == .OK, let url = panel.url else { return }
                try? data.write(to: url)
            }
        }
    }

    extension FocusedValues {
        @Entry var drawingExporter: DrawingExporter?
    }
#endif
