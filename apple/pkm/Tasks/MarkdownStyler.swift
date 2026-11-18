import SwiftUI

#if canImport(UIKit)
    import UIKit
#else
    import AppKit
#endif

extension NSAttributedString.Key {
    /// What to draw in place of the characters it covers, when it is as long as they are: a bullet
    /// for `-`, the number a list has reached. The text itself stays as typed.
    static let markerDisplay = NSAttributedString.Key("pkm.markerDisplay")
}

/// Live Preview: styles a Description from the core's parse. Every line is rendered but the raw
/// range of the caret, where the markers show.
enum MarkdownStyler {
    static var baseFont: PlatformFont { .preferredFont(forTextStyle: .body) }

    static var baseAttributes: [NSAttributedString.Key: Any] {
        [.font: baseFont, .foregroundColor: PlatformColor.label]
    }

    /// Replaces the attributes of `text` with its styling for a caret at `caret` (UTF-16); without
    /// one, as while the text isn't being edited, every line is rendered.
    static func style(_ text: NSMutableAttributedString, caret: Int?) {
        let source = text.string
        let all = NSRange(location: 0, length: text.length)
        text.beginEditing()
        defer { text.endEditing() }
        text.setAttributes(baseAttributes, range: all)
        var caretLine = NSRange(location: NSNotFound, length: 0)
        if let caret {
            let raw = rawRange(text: source, caret: UInt32(clamping: caret))
            caretLine = NSRange(raw.start, raw.end)
        }
        let description = parseDescription(text: source)

        func isRaw(_ range: NSRange) -> Bool {
            caretLine.location <= range.location && range.upperBound <= caretLine.upperBound
        }
        func fade(_ range: NSRange, hidden: Bool) {
            guard range.length > 0 else { return }
            if hidden {
                text.addAttributes(
                    [.font: PlatformFont.systemFont(ofSize: 0.01), .foregroundColor: PlatformColor.clear], range: range)
            } else {
                text.addAttribute(.foregroundColor, value: PlatformColor.secondaryLabel, range: range)
            }
        }

        for block in description.blocks {
            let line = NSRange(block.start, block.end)
            let marker = NSRange(block.markerStart, block.markerEnd)
            let hidden = !isRaw(line)
            switch block.kind {
            case .paragraph, .image:
                break
            case .heading:
                text.addAttribute(.font, value: headingFont(level: Int(block.level)), range: line)
                #if canImport(UIKit)
                    text.addAttribute(.accessibilityTextHeadingLevel, value: Int(block.level), range: line)
                #endif
                fade(marker, hidden: hidden)
            case .quote:
                let style = NSMutableParagraphStyle()
                style.firstLineHeadIndent = 12
                style.headIndent = 12
                text.addAttributes(
                    [.paragraphStyle: style, .foregroundColor: PlatformColor.secondaryLabel], range: line)
                fade(marker, hidden: hidden)
            case .bullet, .numbered:
                text.addAttribute(.foregroundColor, value: PlatformColor.secondaryLabel, range: marker)
                if !hidden { break }
                let shown = block.kind == .bullet ? "•" : "\(block.number)."
                // Only the marker's own characters, not the space after it.
                let characters = NSRange(marker.location, marker.upperBound - 1)
                if shown.utf16.count == characters.length {
                    text.addAttribute(.markerDisplay, value: shown, range: characters)
                }
            case .codeFence:
                text.addAttributes(
                    [
                        .font: monospaced(baseFont), .backgroundColor: PlatformColor.fenceTint,
                    ], range: line)
                text.addAttribute(
                    .foregroundColor, value: PlatformColor.secondaryLabel, range: NSRange(block.start, block.markerEnd))
            }
        }

        for span in description.inlines {
            let range = NSRange(span.start, span.end)
            switch span.kind {
            case .emphasis: transformFont(of: text, in: range) { $0.adding(.italic) }
            case .strong: transformFont(of: text, in: range) { $0.adding(.bold) }
            case .code:
                transformFont(of: text, in: range) { monospaced($0) }
                text.addAttribute(.backgroundColor, value: PlatformColor.fenceTint, range: range)
            }
            let markers = [
                NSRange(location: range.location, length: Int(span.markerLen)),
                NSRange(location: range.upperBound - Int(span.markerLen), length: Int(span.markerLen)),
            ]
            for marker in markers { fade(marker, hidden: !isRaw(range)) }
        }

        for link in description.links {
            let range = NSRange(link.start, link.end)
            let rendered = !isRaw(range)
            fade(NSRange(link.start, link.textStart), hidden: rendered)
            fade(NSRange(link.textEnd, link.end), hidden: rendered)
            // Raw, it is plain text: a tap puts the caret in it instead of opening it.
            guard rendered, let url = URL(string: link.target) else { continue }
            text.addAttribute(.link, value: url, range: NSRange(link.textStart, link.textEnd))
        }
    }

    private static func headingFont(level: Int) -> PlatformFont {
        let scale: CGFloat = [1: 1.5, 2: 1.3, 3: 1.15][level] ?? 1
        return PlatformFont.systemFont(ofSize: baseFont.pointSize * scale, weight: .bold)
    }

    private static func monospaced(_ font: PlatformFont) -> PlatformFont {
        let mono = PlatformFont.monospacedSystemFont(ofSize: font.pointSize, weight: .regular)
        return mono.adding(traits: font.traits)
    }

    /// Outer spans come first, so an inner one adds to what its outer set.
    private static func transformFont(
        of text: NSMutableAttributedString, in range: NSRange, _ change: (PlatformFont) -> PlatformFont
    ) {
        text.enumerateAttribute(.font, in: range) { value, run, _ in
            text.addAttribute(.font, value: change((value as? PlatformFont) ?? baseFont), range: run)
        }
    }
}

extension NSRange {
    init<T: BinaryInteger>(_ start: T, _ end: T) {
        self.init(location: Int(start), length: Int(end - start))
    }
}

/// Draws what `markerDisplay` asks for in place of the text, which stays as typed.
final class MarkerDisplay: NSObject, NSTextContentStorageDelegate {
    func textContentStorage(_ storage: NSTextContentStorage, textParagraphWith range: NSRange) -> NSTextParagraph? {
        guard let source = storage.textStorage else { return nil }
        let paragraph = NSMutableAttributedString(attributedString: source.attributedSubstring(from: range))
        var changed = false
        paragraph.enumerateAttribute(.markerDisplay, in: NSRange(location: 0, length: paragraph.length)) {
            value, run, _ in
            guard let shown = value as? String, shown.utf16.count == run.length else { return }
            paragraph.replaceCharacters(in: run, with: shown)
            changed = true
        }
        return changed ? NSTextParagraph(attributedString: paragraph) : nil
    }
}

extension PlatformColor {
    #if !canImport(UIKit)
        static var label: NSColor { .labelColor }
        static var secondaryLabel: NSColor { .secondaryLabelColor }
    #endif

    static var fenceTint: PlatformColor {
        #if canImport(UIKit)
            .secondarySystemFill
        #else
            .quaternarySystemFill
        #endif
    }
}

extension PlatformFont {
    enum Trait { case bold, italic }

    #if canImport(UIKit)
        typealias Traits = UIFontDescriptor.SymbolicTraits
    #else
        typealias Traits = NSFontDescriptor.SymbolicTraits
    #endif

    #if canImport(UIKit)
        private static let (boldTrait, italicTrait): (Traits, Traits) = (.traitBold, .traitItalic)
    #else
        private static let (boldTrait, italicTrait): (Traits, Traits) = (.bold, .italic)
    #endif

    var traits: Traits { fontDescriptor.symbolicTraits.intersection([Self.boldTrait, Self.italicTrait]) }

    func adding(_ trait: Trait) -> PlatformFont { adding(traits: trait == .bold ? Self.boldTrait : Self.italicTrait) }

    func adding(traits added: Traits) -> PlatformFont {
        let all = fontDescriptor.symbolicTraits.union(added)
        #if canImport(UIKit)
            return fontDescriptor.withSymbolicTraits(all).map { PlatformFont(descriptor: $0, size: pointSize) } ?? self
        #else
            return PlatformFont(descriptor: fontDescriptor.withSymbolicTraits(all), size: pointSize) ?? self
        #endif
    }
}
