import CoreText
import Foundation

#if os(macOS)
    import AppKit
    typealias PlatformFont = NSFont
    typealias PlatformColor = NSColor
    typealias PlatformView = NSView
#else
    import UIKit
    typealias PlatformFont = UIFont
    typealias PlatformColor = UIColor
    typealias PlatformView = UIView
#endif

#if os(macOS)
    /// AppKit's text view, which also ends the editing on Escape, as on
    /// excalidraw.com.
    final class TextEditorTextView: NSTextView {
        var onEscape: (() -> Void)?

        override func cancelOperation(_ sender: Any?) { onEscape?() }
    }
#else
    /// UIKit's text view, which gives the system keyboard, autocorrect,
    /// dictation and, on iPad, Scribble. It takes touches a little beyond its
    /// edges: a line of text is too small a target to write on by hand.
    final class TextEditorTextView: UITextView {
        /// How far beyond its edges the view takes touches, in points: half
        /// of Apple's touch target.
        static let reach: CGFloat = 22

        override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
            bounds.insetBy(dx: -Self.reach, dy: -Self.reach).contains(point)
        }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if presses.contains(where: { $0.key?.keyCode == .keyboardEscape }) {
                resignFirstResponder()
            } else {
                super.pressesBegan(presses, with: event)
            }
        }
    }
#endif

/// The native text view shown over the Canvas while a text is edited (ADR-0006),
/// like Excalidraw's textarea. The core says where it is and what it looks
/// like (`TextEditor`, in the Canvas's coordinates); this keeps a text view
/// there, and reports what is typed and when the editing ends.
///
/// The view is the box of the text, so the text sits where the core draws it
/// once the editing is over: the same Excalifont at the font size times the
/// zoom, lines of the same height, no padding. It has a little room beside
/// the text for the caret at the end of a line.
@MainActor
final class TextEditorOverlay: NSObject {
    /// Room on each side of the text, in points, for the caret.
    static let caretRoom: CGFloat = 6

    /// What the view says, after every change.
    var onChange: ((String) -> Void)?
    /// The editing ended from the view: it lost the focus (the keyboard was
    /// dismissed, another control was used) or Escape was pressed.
    var onEnd: (() -> Void)?

    /// The text view, while a text is edited.
    private(set) var view: TextEditorTextView?
    private var shown: TextEditor?

    /// The text view says.
    var text: String {
        get {
            #if os(macOS)
                view?.string ?? ""
            #else
                view?.text ?? ""
            #endif
        }
        set {
            #if os(macOS)
                view?.string = newValue
            #else
                view?.text = newValue
            #endif
        }
    }

    /// Shows the text view as `editor` says, in `host`: opens it, with the focus,
    /// if it is not open yet. What is typed in it stays; only where it is and
    /// how it looks follows the editor.
    func show(_ editor: TextEditor, in host: PlatformView) {
        guard editor != shown else { return }
        shown = editor
        let view = self.view ?? open(with: editor.text, in: host)
        place(view, as: editor)
    }

    /// Takes the text view away.
    func hide() {
        guard let view else { return }
        // Taking the focus away is not the editing ending from the view.
        #if os(macOS)
            view.delegate = nil
            view.onEscape = nil
        #else
            view.delegate = nil
        #endif
        view.removeFromSuperview()
        self.view = nil
        shown = nil
    }

    // MARK: Opening

    private func open(with text: String, in host: PlatformView) -> TextEditorTextView {
        // The text views need the font by name, which only a registered font has.
        Excalifont.shared?.registerForTextViews()
        #if os(macOS)
            let view = TextEditorTextView(frame: .zero)
            view.delegate = self
            view.onEscape = { [weak self] in self?.onEnd?() }
            view.isRichText = false
            view.drawsBackground = false
            view.isHorizontallyResizable = false
            view.isVerticallyResizable = false
            view.textContainer?.lineFragmentPadding = 0
            view.textContainer?.widthTracksTextView = true
            view.textContainerInset = NSSize(width: Self.caretRoom, height: 0)
            view.setAccessibilityLabel(String(localized: "Text"))
        #else
            let view = TextEditorTextView(frame: .zero, textContainer: nil)
            view.delegate = self
            view.backgroundColor = .clear
            view.isScrollEnabled = false
            view.textContainer.lineFragmentPadding = 0
            view.textContainerInset = UIEdgeInsets(top: 0, left: Self.caretRoom, bottom: 0, right: Self.caretRoom)
            view.accessibilityLabel = String(localized: "Text")
        #endif
        self.view = view
        self.text = text
        host.addSubview(view)
        focus(view)
        return view
    }

    private func focus(_ view: TextEditorTextView) {
        #if os(macOS)
            view.window?.makeFirstResponder(view)
        #else
            view.becomeFirstResponder()
        #endif
    }

    // MARK: Placing

    /// Puts the view where `editor` says and sets the text in the look it says.
    private func place(_ view: TextEditorTextView, as editor: TextEditor) {
        let size = CGSize(width: editor.width + 2 * Self.caretRoom, height: editor.height)
        let origin = CGPoint(x: editor.x - Self.caretRoom, y: editor.y)
        let attributes = Self.attributes(of: editor)
        #if os(macOS)
            view.frameCenterRotation = 0
            view.frame = CGRect(origin: origin, size: size)
            // The core turns clockwise on the screen, AppKit counterclockwise.
            view.frameCenterRotation = -editor.angle * 180 / .pi
            view.textContainer?.lineBreakMode = editor.fixedWidth ? .byWordWrapping : .byClipping
            view.typingAttributes = attributes
            view.textStorage?.addAttributes(attributes, range: NSRange(location: 0, length: view.string.utf16.count))
        #else
            view.transform = .identity
            view.bounds = CGRect(origin: .zero, size: size)
            view.center = CGPoint(x: origin.x + size.width / 2, y: origin.y + size.height / 2)
            view.transform = CGAffineTransform(rotationAngle: editor.angle)
            view.textContainer.lineBreakMode = editor.fixedWidth ? .byWordWrapping : .byClipping
            view.typingAttributes = attributes
            view.textStorage.addAttributes(attributes, range: NSRange(location: 0, length: view.textStorage.length))
        #endif
    }

    /// How the text of `editor` looks: Excalifont at the font size, lines of
    /// exactly the line height, aligned and coloured as the Element is.
    static func attributes(of editor: TextEditor) -> [NSAttributedString.Key: Any] {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = Self.alignment(of: editor.align)
        paragraph.minimumLineHeight = editor.fontSize * editor.lineHeight
        paragraph.maximumLineHeight = editor.fontSize * editor.lineHeight
        return [
            .font: font(size: editor.fontSize),
            .foregroundColor: color(of: editor),
            .paragraphStyle: paragraph,
        ]
    }

    /// Excalifont, or the system font should the app lack it.
    static func font(size: CGFloat) -> PlatformFont {
        Excalifont.shared.flatMap { PlatformFont(name: $0.postScriptName, size: size) }
            ?? PlatformFont.systemFont(ofSize: size)
    }

    private static func alignment(of align: TextAlign) -> NSTextAlignment {
        switch align {
        case .left: .left
        case .center: .center
        case .right: .right
        }
    }

    private static func color(of editor: TextEditor) -> PlatformColor {
        let rgba = editor.color
        return PlatformColor(
            red: CGFloat((rgba >> 24) & 0xFF) / 255, green: CGFloat((rgba >> 16) & 0xFF) / 255,
            blue: CGFloat((rgba >> 8) & 0xFF) / 255,
            alpha: CGFloat(rgba & 0xFF) / 255 * editor.opacity)
    }

    // MARK: Reporting

    /// Tells what the view says now; the view's delegate calls it on every change.
    func changed() { onChange?(text) }
}

#if os(macOS)
    extension TextEditorOverlay: NSTextViewDelegate {
        func textDidChange(_ notification: Notification) { changed() }
        func textDidEndEditing(_ notification: Notification) { onEnd?() }
    }
#else
    extension TextEditorOverlay: UITextViewDelegate {
        func textViewDidChange(_ textView: UITextView) { changed() }
        func textViewDidEndEditing(_ textView: UITextView) { onEnd?() }
    }
#endif
