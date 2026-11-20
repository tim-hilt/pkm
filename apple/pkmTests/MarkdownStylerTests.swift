import Foundation
import Testing

@testable import pkm

#if canImport(UIKit)
    import UIKit
#else
    import AppKit
#endif

/// Live Preview styling of a Description; the parsing itself is the core's.
@Suite struct MarkdownStylerTests {
    private func displayed(_ source: String, caret: Int?) -> String {
        let text = NSMutableAttributedString(string: source)
        MarkdownStyler.style(text, caret: caret)
        let paragraph = MarkerDisplay().paragraph(of: text)
        return paragraph
    }

    @Test func ligaturesShowAsGlyphsOfTheSameLengthOnLinesWithoutTheCaret() {
        let shown = displayed("a -> b\nc ==> d", caret: 0)  // caret in line one
        #expect(shown.hasPrefix("a -> b\n"))
        #expect(shown.utf16.count == "a -> b\nc ==> d".utf16.count)
        #expect(shown.contains("⟹"))
        #expect(displayed("a -> b `c->d`", caret: nil).contains("→"))
        #expect(!displayed("`c->d`", caret: nil).contains("→"))
    }
}

extension MarkerDisplay {
    fileprivate func paragraph(of text: NSAttributedString) -> String {
        let storage = NSTextContentStorage()
        storage.textStorage?.setAttributedString(text)
        let range = NSRange(location: 0, length: text.length)
        return textContentStorage(storage, textParagraphWith: range)?.attributedString.string ?? text.string
    }
}
