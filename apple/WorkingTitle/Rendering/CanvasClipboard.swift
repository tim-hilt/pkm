import Foundation
import UniformTypeIdentifiers

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// Copying, cutting and pasting Elements. The text on the pasteboard is
/// Excalidraw's clipboard JSON (`excalidraw/clipboard`) as plain text, which
/// the core writes and reads, so Elements move between this app and
/// excalidraw.com. What is on the pasteboard but is not that (or an image,
/// see `CanvasImageInput`) is not ours to paste.
extension CanvasMetalView {
    /// The Selection as clipboard text, which `cutting` also deletes from the
    /// Scene; `nil` with nothing selected.
    func clipboardText(cutting: Bool) -> String? {
        var text: String?
        perform { text = cutting ? $0.cutSelection() : $0.copySelection() }
        return text
    }

    /// Whether there is a Selection to copy or cut.
    var canCopy: Bool { editor?.selection().isEmpty == false }

    /// Pastes `text` if it is Excalidraw clipboard JSON: around `point` (in
    /// the Canvas's coordinates) where it was dropped, else in the middle of
    /// the view. Returns whether it was.
    @discardableResult
    func pasteClipboardText(_ text: String, at point: CGPoint? = nil) -> Bool {
        var pasted = false
        perform { editor in
            pasted =
                if let point {
                    editor.pasteAt(text: text, x: point.x, y: point.y)
                } else {
                    editor.paste(text: text)
                }
        }
        return pasted
    }

    #if os(macOS)
        /// Edit > Copy (⌘C) while the Canvas has the keyboard.
        @objc func copy(_ sender: Any?) { writeToPasteboard(cutting: false) }

        /// Edit > Cut (⌘X) while the Canvas has the keyboard.
        @objc func cut(_ sender: Any?) { writeToPasteboard(cutting: true) }

        private func writeToPasteboard(cutting: Bool) {
            guard let text = clipboardText(cutting: cutting) else { return }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        }

        /// The Elements on the pasteboard, if any.
        func pasteElementsFromPasteboard() -> Bool {
            guard let text = NSPasteboard.general.string(forType: .string) else { return false }
            return pasteClipboardText(text)
        }
    #else
        override func copy(_ sender: Any?) { writeToPasteboard(cutting: false) }

        override func cut(_ sender: Any?) { writeToPasteboard(cutting: true) }

        private func writeToPasteboard(cutting: Bool) {
            guard let text = clipboardText(cutting: cutting) else { return }
            UIPasteboard.general.string = text
        }

        override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
            switch action {
            case #selector(copy(_:)), #selector(cut(_:)): canCopy
            default: super.canPerformAction(action, withSender: sender)
            }
        }

        /// The Elements among the pasted `providers`, if any.
        func pasteElements(from providers: [NSItemProvider]) async -> Bool {
            for provider in providers where provider.canLoadObject(ofClass: NSString.self) {
                let text = await withCheckedContinuation { continuation in
                    _ = provider.loadObject(ofClass: NSString.self) { text, _ in
                        continuation.resume(returning: text as? String)
                    }
                }
                if let text, pasteClipboardText(text) { return true }
            }
            return false
        }
    #endif
}
