import Foundation
import UniformTypeIdentifiers

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// Images that reach the Canvas from outside: pasted from the clipboard and
/// dragged & dropped onto it. Every route ends in `insertImages`, which has
/// the pictures prepared for the core (`ImageImporter`) and hands them over.
extension CanvasMetalView {
    /// Puts the images in `sources` (the bytes of image files, in any format
    /// ImageIO reads) on the Canvas: around `point` (in the Canvas's
    /// coordinates) where they were dropped, else in the middle of the view.
    /// What is not an image is skipped. The pictures are decoded and shrunk
    /// off the main thread.
    func insertImages(from sources: [Data], at point: CGPoint? = nil) async {
        let files = await Task.detached { sources.compactMap(ImageImporter.prepare) }.value
        guard !files.isEmpty else { return }
        perform { editor in
            if let point {
                editor.insertImagesAt(images: files, x: point.x, y: point.y)
            } else {
                editor.insertImages(images: files)
            }
        }
    }

    #if os(macOS)
        /// Makes the Canvas take drops of images.
        func enableImageInput() {
            registerForDraggedTypes([.fileURL, .png, .tiff])
        }

        /// Edit > Paste (⌘V) while the Canvas has the keyboard.
        @objc func paste(_ sender: Any?) {
            let sources = Self.imageSources(in: .general)
            Task { await insertImages(from: sources) }
        }

        override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
            Self.imageSources(in: sender.draggingPasteboard).isEmpty ? [] : .copy
        }

        override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
            let sources = Self.imageSources(in: sender.draggingPasteboard)
            guard !sources.isEmpty else { return false }
            let point = convert(sender.draggingLocation, from: nil)
            Task { await insertImages(from: sources, at: point) }
            return true
        }

        /// The images on a pasteboard: the files copied or dragged from the
        /// Finder if they are images, else the picture itself (what a
        /// screenshot is).
        fileprivate static func imageSources(in pasteboard: NSPasteboard) -> [Data] {
            let urls =
                pasteboard.readObjects(
                    forClasses: [NSURL.self],
                    options: [
                        .urlReadingFileURLsOnly: true,
                        .urlReadingContentsConformToTypes: [UTType.image.identifier],
                    ]) as? [URL] ?? []
            let files = urls.compactMap { try? Data(contentsOf: $0) }
            if !files.isEmpty { return files }
            return [NSPasteboard.PasteboardType.png, .tiff].lazy.compactMap { pasteboard.data(forType: $0) }
                .first.map { [$0] } ?? []
        }
    #else
        /// Makes the Canvas take drops and pastes of images.
        func enableImageInput() {
            addInteraction(UIDropInteraction(delegate: self))
            pasteConfiguration = UIPasteConfiguration(acceptableTypeIdentifiers: [UTType.image.identifier])
        }

        override var canBecomeFirstResponder: Bool { true }

        /// A paste: ⌘V with a keyboard, or a paste button (`UIPasteControl`).
        override func paste(itemProviders: [NSItemProvider]) {
            Task { await insertImages(from: await Self.imageData(of: itemProviders)) }
        }

        fileprivate static func imageData(of providers: [NSItemProvider]) async -> [Data] {
            var sources: [Data] = []
            for provider in providers where provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
                let data = await withCheckedContinuation { continuation in
                    provider.loadDataRepresentation(forTypeIdentifier: UTType.image.identifier) { data, _ in
                        continuation.resume(returning: data)
                    }
                }
                if let data { sources.append(data) }
            }
            return sources
        }
    #endif
}

#if os(macOS)
    extension CanvasMetalView: NSMenuItemValidation {
        /// Paste is there when the clipboard has an image.
        func validateMenuItem(_ item: NSMenuItem) -> Bool {
            item.action != #selector(paste(_:)) || !Self.imageSources(in: .general).isEmpty
        }
    }
#else
    extension CanvasMetalView: UIDropInteractionDelegate {
        func dropInteraction(_ interaction: UIDropInteraction, canHandle session: any UIDropSession) -> Bool {
            session.hasItemsConforming(toTypeIdentifiers: [UTType.image.identifier])
        }

        func dropInteraction(
            _ interaction: UIDropInteraction, sessionDidUpdate session: any UIDropSession
        ) -> UIDropProposal {
            UIDropProposal(operation: .copy)
        }

        func dropInteraction(_ interaction: UIDropInteraction, performDrop session: any UIDropSession) {
            let point = session.location(in: self)
            let providers = session.items.map(\.itemProvider)
            Task { await insertImages(from: await Self.imageData(of: providers), at: point) }
        }
    }
#endif
