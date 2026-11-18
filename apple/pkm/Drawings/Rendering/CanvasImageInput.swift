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
            registerForDraggedTypes(PictureInput.droppedTypes)
        }

        /// Edit > Paste (⌘V) while the Canvas has the keyboard: at the mouse
        /// pointer, as excalidraw.com pastes on a desktop, or in the middle
        /// of the view if the pointer is not over the Canvas.
        @objc func paste(_ sender: Any?) {
            let point = pointerLocation
            // Elements first: Excalidraw's clipboard is text.
            if pasteElementsFromPasteboard(at: point) { return }
            let sources = PictureInput.pictures(in: .general)
            Task { await insertImages(from: sources, at: point) }
        }

        override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
            PictureInput.hasPictures(in: sender.draggingPasteboard) ? .copy : []
        }

        override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
            let sources = PictureInput.pictures(in: sender.draggingPasteboard)
            guard !sources.isEmpty else { return false }
            let point = convert(sender.draggingLocation, from: nil)
            Task { await insertImages(from: sources, at: point) }
            return true
        }
    #else
        /// Makes the Canvas take drops and pastes of images.
        func enableImageInput() {
            addInteraction(UIDropInteraction(delegate: self))
            pasteConfiguration = UIPasteConfiguration(acceptableTypeIdentifiers: Self.pasteTypes)
        }

        /// What can be pasted: images, and the text Excalidraw's Elements are.
        static let pasteTypes = [UTType.image.identifier, UTType.plainText.identifier]

        override var canBecomeFirstResponder: Bool { true }

        /// A paste: ⌘V with a keyboard, or the Paste item of the Context menu,
        /// which puts it around where the menu was opened.
        override func paste(itemProviders: [NSItemProvider]) {
            let point = contextMenuPoint
            Task {
                if await pasteElements(from: itemProviders, at: point) { return }
                await insertImages(from: await PictureInput.pictures(of: itemProviders), at: point)
            }
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
                NSPasteboard.general.availableType(from: [.string]) != nil || PictureInput.hasPictures(in: .general)
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
            Task { await insertImages(from: await PictureInput.pictures(of: providers), at: point) }
        }
    }
#endif
