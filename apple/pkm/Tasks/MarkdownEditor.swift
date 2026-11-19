import SwiftUI
import UniformTypeIdentifiers

/// A Description edited in Live Preview (ADR-0028): a TextKit 2 text view whose lines are styled
/// from the core's parse (`MarkdownStyler`), the caret's line raw. The text is exactly what was
/// typed. It grows with its content, so the sheet around it scrolls. Links open on a tap or click,
/// and pasting takes only the plain text, or a picture, which `images` keeps as an Image. While
/// focused it offers ⌘B, ⌘I and ⌘E (`FormatCommands`).
struct MarkdownEditor: View {
    @Binding var text: String
    var placeholder: LocalizedStringKey
    /// Where the Images are; without it, pictures can't be pasted or dropped and Images stay raw.
    var images: ImageSource?
    /// The editor lost focus.
    var onBlur: () -> Void = {}
    /// Formats the selection; set while the editor has the focus.
    @State private var formatting: ((Inline) -> Void)?

    var body: some View {
        MarkdownTextRepresentable(text: $text, formatting: $formatting, images: images, onBlur: onBlur)
            .focusedSceneValue(\.formatText, formatting)
            .accessibilityLabel(placeholder)
            .overlay(alignment: .topLeading) {
                if text.isEmpty {
                    Text(placeholder).foregroundStyle(.tertiary).allowsHitTesting(false)
                }
            }
    }
}

/// The keys the core edits for (ADR-0028): Return continues a list, Tab and Shift-Tab indent and
/// outdent its items, and the format shortcuts toggle Inline formatting. An Image pasted or dropped
/// is the same kind of edit.
private enum EditKey {
    case newline, indent, outdent
    case toggle(Inline)
    case image(reference: String)

    /// The edit for the key with `selection` (UTF-16) selected in `text`; none to leave the key to
    /// the text view, as for Tab outside a list.
    func edit(in text: String, selection: NSRange) -> TextEdit? {
        let start = UInt32(clamping: selection.location)
        let end = UInt32(clamping: selection.upperBound)
        switch self {
        case .newline: return returnAt(text: text, start: start, end: end)
        case .indent: return shiftItems(text: text, start: start, end: end, outdent: false)
        case .outdent: return shiftItems(text: text, start: start, end: end, outdent: true)
        case .toggle(let kind): return toggleInline(text: text, start: start, end: end, kind: kind)
        case .image(let reference): return insertImage(text: text, start: start, end: end, reference: reference)
        }
    }
}

extension TextEdit {
    fileprivate var replaced: NSRange { NSRange(start, end) }
    fileprivate var selection: NSRange { NSRange(selectionStart, selectionEnd) }
}

/// At least as high as this many lines of text.
private let minimumLines: CGFloat = 3

extension NSTextStorage {
    /// The link at `point` (in the text container) if the character at `index` or the one before is
    /// one and `point` is on it: a tap past the end of a line is not on its last link.
    fileprivate func link(at point: CGPoint, near index: Int, in layout: NSTextLayoutManager?) -> URL? {
        guard let layout, let content = layout.textContentManager else { return nil }
        for i in [index - 1, index] where i >= 0 && i < length {
            guard let url = attribute(.link, at: i, effectiveRange: nil) as? URL,
                let start = content.location(content.documentRange.location, offsetBy: i),
                let end = content.location(start, offsetBy: 1),
                let range = NSTextRange(location: start, end: end)
            else { continue }
            var hit = false
            layout.enumerateTextSegments(in: range, type: .standard, options: []) { _, frame, _, _ in
                hit = hit || frame.contains(point)
                return true
            }
            if hit { return url }
        }
        return nil
    }
}

#if canImport(UIKit)
    private final class MarkdownTextView: UITextView {
        /// An edit is going in, whose own line break the delegate must not take for a Return.
        private var isApplying = false
        var images: ImageSource?

        override func paste(_ sender: Any?) {
            if images != nil, let picture = PictureInput.picture(in: .general) {
                insert(picture)
            } else if let text = UIPasteboard.general.string {
                insertText(text)
            }
        }

        override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
            if action == #selector(paste(_:)), images != nil, UIPasteboard.general.hasImages { return true }
            return super.canPerformAction(action, withSender: sender)
        }

        /// Keeps `picture` as an Image and puts its line at the selection.
        func insert(_ picture: Data) {
            guard let reference = images?.add(picture) else { return }
            _ = apply(.image(reference: reference))
        }

        /// Tab and Shift-Tab of a hardware keyboard, which the text view would take as focus keys.
        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            let tab = presses.first { $0.key?.keyCode == .keyboardTab }
            if let tab, let key = tab.key,
                apply(key.modifierFlags.contains(.shift) ? .outdent : .indent)
            {
                return
            }
            super.pressesBegan(presses, with: event)
        }

        /// Applies the core's edit for `key` as a typed change, so that undo takes it back.
        func apply(_ key: EditKey) -> Bool {
            guard !isApplying, let edit = key.edit(in: text, selection: selectedRange),
                let from = position(from: beginningOfDocument, offset: Int(edit.start)),
                let to = position(from: beginningOfDocument, offset: Int(edit.end)),
                let range = textRange(from: from, to: to)
            else { return false }
            isApplying = true
            replace(range, withText: edit.replacement)
            isApplying = false
            selectedRange = edit.selection
            return true
        }
    }

    private struct MarkdownTextRepresentable: UIViewRepresentable {
        @Binding var text: String
        @Binding var formatting: ((Inline) -> Void)?
        let images: ImageSource?
        let onBlur: () -> Void

        func makeCoordinator() -> Coordinator { Coordinator(self) }

        func makeUIView(context: Context) -> UITextView {
            let view = MarkdownTextView(usingTextLayoutManager: true)
            let coordinator = context.coordinator
            view.delegate = coordinator
            view.images = images
            if images != nil { view.addInteraction(UIDropInteraction(delegate: coordinator)) }
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate = coordinator.display
            view.backgroundColor = .clear
            view.isScrollEnabled = false
            view.textContainerInset = .zero
            view.textContainer.lineFragmentPadding = 0
            view.smartDashesType = .no
            view.smartQuotesType = .no
            view.typingAttributes = MarkdownStyler.baseAttributes
            let tap = UITapGestureRecognizer(target: coordinator, action: #selector(Coordinator.tapped))
            tap.delegate = coordinator
            tap.cancelsTouchesInView = false
            view.addGestureRecognizer(tap)
            return view
        }

        func updateUIView(_ view: UITextView, context: Context) {
            context.coordinator.parent = self
            guard view.text != text else { return }
            context.coordinator.isUpdating = true
            view.text = text
            context.coordinator.isUpdating = false
            context.coordinator.restyle(view)
        }

        func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextView, context: Context) -> CGSize? {
            let width = proposal.width ?? 320
            let fitted = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
            return CGSize(width: width, height: max(fitted.height, MarkdownStyler.baseFont.lineHeight * minimumLines))
        }

        final class Coordinator: NSObject, UITextViewDelegate, UIGestureRecognizerDelegate, UIDropInteractionDelegate {
            var parent: MarkdownTextRepresentable
            let display = MarkerDisplay()
            let attachments: ImageAttachments?
            var isUpdating = false
            private var isStyling = false
            /// The link under the touch that is going on, as it was drawn when the touch began.
            private var touched: URL?

            init(_ parent: MarkdownTextRepresentable) {
                self.parent = parent
                attachments = parent.images.map(ImageAttachments.init)
            }

            func dropInteraction(_ interaction: UIDropInteraction, canHandle session: any UIDropSession) -> Bool {
                session.hasItemsConforming(toTypeIdentifiers: [UTType.image.identifier])
            }

            func dropInteraction(
                _ interaction: UIDropInteraction, sessionDidUpdate session: any UIDropSession
            ) -> UIDropProposal {
                UIDropProposal(operation: .copy)
            }

            func dropInteraction(_ interaction: UIDropInteraction, performDrop session: any UIDropSession) {
                guard let view = interaction.view as? MarkdownTextView else { return }
                let point = session.location(in: view)
                let providers = session.items.map(\.itemProvider)
                Task { @MainActor in
                    for picture in await PictureInput.pictures(of: providers) {
                        if let position = view.closestPosition(to: point) {
                            view.selectedRange = NSRange(
                                location: view.offset(from: view.beginningOfDocument, to: position), length: 0)
                        }
                        view.insert(picture)
                    }
                }
            }

            func restyle(_ view: UITextView) {
                guard !isStyling, !isUpdating, view.markedTextRange == nil else { return }
                isStyling = true
                MarkdownStyler.style(
                    view.textStorage, caret: view.isFirstResponder ? view.selectedRange.location : nil,
                    images: attachments)
                view.typingAttributes = MarkdownStyler.baseAttributes
                isStyling = false
            }

            func textView(_ view: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
                guard text == "\n", let view = view as? MarkdownTextView else { return true }
                return !view.apply(.newline)
            }

            func textViewDidChange(_ view: UITextView) {
                parent.text = view.text
                restyle(view)
            }

            func textViewDidChangeSelection(_ view: UITextView) { restyle(view) }

            func textViewDidBeginEditing(_ view: UITextView) {
                parent.formatting = (view as? MarkdownTextView).map(formatter)
                restyle(view)
            }

            func textViewDidEndEditing(_ view: UITextView) {
                parent.formatting = nil
                restyle(view)
                parent.onBlur()
            }

            func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
                touched = nil
                if let view = recognizer.view as? UITextView {
                    let point = touch.location(in: view)
                    if let position = view.closestPosition(to: point) {
                        let index = view.offset(from: view.beginningOfDocument, to: position)
                        touched = view.textStorage.link(at: point, near: index, in: view.textLayoutManager)
                    }
                }
                return true
            }

            func gestureRecognizer(
                _ recognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
            ) -> Bool { true }

            @objc func tapped() {
                if let touched { UIApplication.shared.open(touched) }
                touched = nil
            }
        }
    }
#else
    private final class MarkdownTextView: NSTextView {
        /// Tells whether the view has the focus.
        var onFocusChange: ((Bool) -> Void)?
        var images: ImageSource?

        override func paste(_ sender: Any?) {
            if images != nil, PictureInput.hasPictures(in: .general) {
                insert(PictureInput.pictures(in: .general))
            } else {
                pasteAsPlainText(sender)
            }
        }

        /// Keeps each of `pictures` as an Image and puts its line at the selection.
        private func insert(_ pictures: [Data]) {
            for picture in pictures {
                guard let reference = images?.add(picture) else { continue }
                _ = apply(.image(reference: reference))
            }
        }

        private func takesDrop(_ sender: any NSDraggingInfo) -> Bool {
            images != nil && PictureInput.hasPictures(in: sender.draggingPasteboard)
        }

        override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
            takesDrop(sender) ? .copy : super.draggingEntered(sender)
        }

        override func draggingUpdated(_ sender: any NSDraggingInfo) -> NSDragOperation {
            takesDrop(sender) ? .copy : super.draggingUpdated(sender)
        }

        override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
            guard takesDrop(sender) else { return super.performDragOperation(sender) }
            let index = characterIndexForInsertion(at: convert(sender.draggingLocation, from: nil))
            setSelectedRange(NSRange(location: index, length: 0))
            insert(PictureInput.pictures(in: sender.draggingPasteboard))
            return true
        }

        override func becomeFirstResponder() -> Bool {
            let became = super.becomeFirstResponder()
            if became { onFocusChange?(true) }
            return became
        }

        override func resignFirstResponder() -> Bool {
            let resigned = super.resignFirstResponder()
            if resigned { onFocusChange?(false) }
            return resigned
        }

        /// Applies the core's edit for `key` as a typed change, so that undo takes it back.
        func apply(_ key: EditKey) -> Bool {
            guard let edit = key.edit(in: string, selection: selectedRange()) else { return false }
            insertText(edit.replacement, replacementRange: edit.replaced)
            setSelectedRange(edit.selection)
            return true
        }

        override func mouseDown(with event: NSEvent) {
            let point = convert(event.locationInWindow, from: nil)
            let index = characterIndexForInsertion(at: point)
            let origin = textContainerOrigin
            let inContainer = CGPoint(x: point.x - origin.x, y: point.y - origin.y)
            if let url = textStorage?.link(at: inContainer, near: index, in: textLayoutManager) {
                NSWorkspace.shared.open(url)
                return
            }
            super.mouseDown(with: event)
        }
    }

    private struct MarkdownTextRepresentable: NSViewRepresentable {
        @Binding var text: String
        @Binding var formatting: ((Inline) -> Void)?
        let images: ImageSource?
        let onBlur: () -> Void

        func makeCoordinator() -> Coordinator { Coordinator(self) }

        func makeNSView(context: Context) -> NSTextView {
            let view = MarkdownTextView(usingTextLayoutManager: true)
            let coordinator = context.coordinator
            view.delegate = coordinator
            view.images = images
            if images != nil { view.registerForDraggedTypes(view.registeredDraggedTypes + PictureInput.droppedTypes) }
            view.onFocusChange = { [unowned coordinator, weak view] focused in
                coordinator.parent.formatting = focused ? view.map(formatter) : nil
            }
            (view.textLayoutManager?.textContentManager as? NSTextContentStorage)?.delegate = coordinator.display
            view.drawsBackground = false
            view.isVerticallyResizable = false
            view.isHorizontallyResizable = false
            view.textContainerInset = .zero
            view.textContainer?.lineFragmentPadding = 0
            view.textContainer?.widthTracksTextView = true
            view.allowsUndo = true
            view.isAutomaticQuoteSubstitutionEnabled = false
            view.isAutomaticDashSubstitutionEnabled = false
            view.isAutomaticTextReplacementEnabled = false
            view.importsGraphics = false
            view.typingAttributes = MarkdownStyler.baseAttributes
            return view
        }

        func updateNSView(_ view: NSTextView, context: Context) {
            context.coordinator.parent = self
            guard view.string != text else { return }
            context.coordinator.isUpdating = true
            view.string = text
            context.coordinator.isUpdating = false
            context.coordinator.restyle(view)
        }

        func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextView, context: Context) -> CGSize? {
            let width = proposal.width ?? 320
            nsView.textContainer?.size = CGSize(width: width, height: .greatestFiniteMagnitude)
            guard let layout = nsView.textLayoutManager else { return nil }
            layout.ensureLayout(for: layout.documentRange)
            let height = layout.usageBoundsForTextContainer.height
            return CGSize(
                width: width, height: max(height, MarkdownStyler.baseFont.boundingRectForFont.height * minimumLines))
        }

        final class Coordinator: NSObject, NSTextViewDelegate {
            var parent: MarkdownTextRepresentable
            let display = MarkerDisplay()
            let attachments: ImageAttachments?
            var isUpdating = false
            private var isStyling = false
            private var isEditing = false

            init(_ parent: MarkdownTextRepresentable) {
                self.parent = parent
                attachments = parent.images.map(ImageAttachments.init)
            }

            func restyle(_ view: NSTextView) {
                guard !isStyling, !isUpdating, !view.hasMarkedText(), let storage = view.textStorage else { return }
                isStyling = true
                MarkdownStyler.style(
                    storage, caret: isEditing ? view.selectedRange().location : nil, images: attachments)
                view.typingAttributes = MarkdownStyler.baseAttributes
                isStyling = false
            }

            func textView(_ view: NSTextView, doCommandBy selector: Selector) -> Bool {
                let key: EditKey
                switch selector {
                case #selector(NSResponder.insertNewline(_:)): key = .newline
                case #selector(NSResponder.insertTab(_:)): key = .indent
                case #selector(NSResponder.insertBacktab(_:)): key = .outdent
                default: return false
                }
                return (view as? MarkdownTextView)?.apply(key) ?? false
            }

            func textDidChange(_ notification: Notification) {
                guard let view = notification.object as? NSTextView else { return }
                parent.text = view.string
                restyle(view)
            }

            func textViewDidChangeSelection(_ notification: Notification) {
                guard let view = notification.object as? NSTextView else { return }
                isEditing = view.window?.firstResponder === view
                restyle(view)
            }

            func textDidEndEditing(_ notification: Notification) {
                isEditing = false
                if let view = notification.object as? NSTextView { restyle(view) }
                parent.onBlur()
            }
        }
    }
#endif

/// What formats the selection of `view`, for `FormatCommands`.
private func formatter(for view: MarkdownTextView) -> (Inline) -> Void {
    { [weak view] kind in _ = view?.apply(.toggle(kind)) }
}
