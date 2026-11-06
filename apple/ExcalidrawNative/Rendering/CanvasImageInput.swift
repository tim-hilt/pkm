import Foundation
import UniformTypeIdentifiers

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// Images that reach the Canvas from outside (text, which is Excalidraw's
/// Elements, is `CanvasClipboard`): pasted from the clipboard and
/// dragged & dropped onto it. Every route ends in `insertImages`, which has
/// the pictures prepared for the core (`ImageImporter`) and hands them over.
extension CanvasMetalView {
    /// Puts the images in `sources` (the bytes of image files, in any format
    /// ImageIO reads) on the Canvas: around `point` (in the Canvas's
    /// coordinates) where they were dropped or pasted, else in the middle of the view.
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

        /// Edit > Paste (⌘V) while the Canvas has the keyboard: at the mouse
        /// pointer, as excalidraw.com pastes on a desktop, or in the middle
        /// of the view if the pointer is not over the Canvas.
        @objc func paste(_ sender: Any?) {
            let point = pointerLocation
            // Elements first: Excalidraw's clipboard is text.
            if pasteElementsFromPasteboard(at: point) { return }
            let sources = Self.imageSources(in: .general)
            Task { await insertImages(from: sources, at: point) }
        }

        override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
            Self.hasImages(in: sender.draggingPasteboard) ? .copy : []
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
            let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: imageFiles) as? [URL] ?? []
            let files = urls.compactMap { try? Data(contentsOf: $0) }
            if !files.isEmpty { return files }
            return pictureTypes.lazy.compactMap { pasteboard.data(forType: $0) }.first.map { [$0] } ?? []
        }

        /// Whether `imageSources(in:)` finds anything, without reading it:
        /// asked on every drag movement and menu validation.
        fileprivate static func hasImages(in pasteboard: NSPasteboard) -> Bool {
            pasteboard.canReadObject(forClasses: [NSURL.self], options: imageFiles)
                || pasteboard.availableType(from: pictureTypes) != nil
        }

        /// Files that are images, of a pasteboard.
        private static let imageFiles: [NSPasteboard.ReadingOptionKey: Any] = [
            .urlReadingFileURLsOnly: true,
            .urlReadingContentsConformToTypes: [UTType.image.identifier],
        ]

        /// The picture itself on a pasteboard, best first.
        private static let pictureTypes: [NSPasteboard.PasteboardType] = [.png, .tiff]
    #else
        /// Makes the Canvas take drops and pastes of images.
        func enableImageInput() {
            addInteraction(UIDropInteraction(delegate: self))
            pasteConfiguration = UIPasteConfiguration(acceptableTypeIdentifiers: Self.pasteTypes)
        }

        /// What can be pasted: images, and the text Excalidraw's Elements are.
        static let pasteTypes = [UTType.image.identifier, UTType.plainText.identifier]

        override var canBecomeFirstResponder: Bool { true }

        /// A paste: ⌘V with a keyboard, or a paste button (`UIPasteControl`).
        override func paste(itemProviders: [NSItemProvider]) {
            Task {
                if await pasteElements(from: itemProviders) { return }
                await insertImages(from: await Self.imageData(of: itemProviders))
            }
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
        /// Copy and cut are there with a Selection; paste when the clipboard has
        /// an image or text.
        func validateMenuItem(_ item: NSMenuItem) -> Bool {
            switch item.action {
            case #selector(copy(_:)), #selector(cut(_:)): canCopy
            case #selector(paste(_:)):
                NSPasteboard.general.availableType(from: [.string]) != nil || Self.hasImages(in: .general)
            default: true
            }
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
