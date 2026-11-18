import SwiftUI

/// A Description edited in Live Preview (ADR-0028): a TextKit 2 text view whose lines are styled
/// from the core's parse (`MarkdownStyler`), the caret's line raw. The text is exactly what was
/// typed. It grows with its content, so the sheet around it scrolls. Links open on a tap or click,
/// and pasting takes only the plain text.
struct MarkdownEditor: View {
    @Binding var text: String
    var placeholder: LocalizedStringKey
    /// The editor lost focus.
    var onBlur: () -> Void = {}

    var body: some View {
        MarkdownTextRepresentable(text: $text, onBlur: onBlur)
            .accessibilityLabel(placeholder)
            .overlay(alignment: .topLeading) {
                if text.isEmpty {
                    Text(placeholder).foregroundStyle(.tertiary).allowsHitTesting(false)
                }
            }
    }
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
        override func paste(_ sender: Any?) {
            if let text = UIPasteboard.general.string { insertText(text) }
        }
    }

    private struct MarkdownTextRepresentable: UIViewRepresentable {
        @Binding var text: String
        let onBlur: () -> Void

        func makeCoordinator() -> Coordinator { Coordinator(self) }

        func makeUIView(context: Context) -> UITextView {
            let view = MarkdownTextView(usingTextLayoutManager: true)
            let coordinator = context.coordinator
            view.delegate = coordinator
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

        final class Coordinator: NSObject, UITextViewDelegate, UIGestureRecognizerDelegate {
            var parent: MarkdownTextRepresentable
            let display = MarkerDisplay()
            var isUpdating = false
            private var isStyling = false
            /// The link under the touch that is going on, as it was drawn when the touch began.
            private var touched: URL?

            init(_ parent: MarkdownTextRepresentable) { self.parent = parent }

            func restyle(_ view: UITextView) {
                guard !isStyling, !isUpdating, view.markedTextRange == nil else { return }
                isStyling = true
                MarkdownStyler.style(view.textStorage, caret: view.isFirstResponder ? view.selectedRange.location : nil)
                view.typingAttributes = MarkdownStyler.baseAttributes
                isStyling = false
            }

            func textViewDidChange(_ view: UITextView) {
                parent.text = view.text
                restyle(view)
            }

            func textViewDidChangeSelection(_ view: UITextView) { restyle(view) }

            func textViewDidBeginEditing(_ view: UITextView) { restyle(view) }

            func textViewDidEndEditing(_ view: UITextView) {
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
        override func paste(_ sender: Any?) { pasteAsPlainText(sender) }

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
        let onBlur: () -> Void

        func makeCoordinator() -> Coordinator { Coordinator(self) }

        func makeNSView(context: Context) -> NSTextView {
            let view = MarkdownTextView(usingTextLayoutManager: true)
            let coordinator = context.coordinator
            view.delegate = coordinator
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
            var isUpdating = false
            private var isStyling = false
            private var isEditing = false

            init(_ parent: MarkdownTextRepresentable) { self.parent = parent }

            func restyle(_ view: NSTextView) {
                guard !isStyling, !isUpdating, !view.hasMarkedText(), let storage = view.textStorage else { return }
                isStyling = true
                MarkdownStyler.style(storage, caret: isEditing ? view.selectedRange().location : nil)
                view.typingAttributes = MarkdownStyler.baseAttributes
                isStyling = false
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
