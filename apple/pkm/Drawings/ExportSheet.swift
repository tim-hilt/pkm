import SwiftUI

/// What the Drawing in front offers the menus (menu bar, main menu): Save As
/// (the `.excalidraw` file) and Export (the image).
struct DrawingFile {
    let saveAs: () -> Void
    let export: () -> Void
}

extension FocusedValues {
    @Entry var drawingFile: DrawingFile?
}

/// Export, as excalidraw.com's Export image dialog: the preview of the image,
/// and buttons to save it or copy it to the clipboard.
struct ExportSheet: View {
    let exported: ExportedImage
    @State private var isSaving = false
    @State private var copied = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Export Image").font(.title2.bold())
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            preview
            HStack {
                Button("Save PNG…", systemImage: "square.and.arrow.down") { isSaving = true }
                    .buttonStyle(.borderedProminent)
                Button(
                    copied ? "Copied" : "Copy to Clipboard", systemImage: copied ? "checkmark" : "doc.on.doc"
                ) {
                    exported.copyToPasteboard()
                    copied = true
                }
                .buttonStyle(.bordered)
            }
        }
        .padding(20)
        #if os(macOS)
            .frame(width: 560)
        #endif
        .fileExporter(
            isPresented: $isSaving, item: exported, contentTypes: [.png], defaultFilename: exported.name
        ) { _ in }
    }

    /// The image at its size in points (2 pixels each), shrunk to fit.
    @ViewBuilder private var preview: some View {
        if let image = exported.image.cgImage() {
            Image(image, scale: CGFloat(PNGExport.scale), label: Text("Preview"))
                .resizable()
                .scaledToFit()
                .frame(
                    maxWidth: CGFloat(exported.image.width) / CGFloat(PNGExport.scale),
                    maxHeight: CGFloat(exported.image.height) / CGFloat(PNGExport.scale)
                )
                .shadow(color: .black.opacity(0.25), radius: 2)
                .padding()
                .frame(maxWidth: .infinity, minHeight: 200, maxHeight: 360)
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.separator))
        }
    }
}
