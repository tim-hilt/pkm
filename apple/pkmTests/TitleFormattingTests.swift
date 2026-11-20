import Foundation
import Testing

@testable import pkm

/// A Title drawn with its Inline formatting; the parsing itself is the core's.
@Suite struct TitleFormattingTests {
    @Test func removedMarkersLeaveTheTextBetweenThem() {
        let title = AttributedString(title: "a *b* **c** `d` 😀")
        #expect(String(title.characters) == "a b c d 😀")
    }

    @Test func ligaturesAreDrawnAsOneGlyphWhereMarkersAreRemoved() {
        #expect(String(AttributedString(title: "a -> *b* `c->d`").characters) == "a → b c->d")
        #expect(String(AttributedString(title: "a -> b", markers: .hidden).characters) == "a -> b")
    }

    @Test func shownMarkersKeepTheTextAsTyped() {
        let title = AttributedString(title: "a *b* **c**", markers: .shown)
        #expect(String(title.characters) == "a *b* **c**")
    }

    @Test func hiddenMarkersKeepTheLengthForAnEditor() {
        let title = AttributedString(title: "*b*", markers: .hidden)
        #expect(String(title.characters) == "*b*")
    }

    @Test func nestedFormattingAddsUp() {
        let title = AttributedString(title: "***x***")
        #expect(title.runs.first?.inlinePresentationIntent == [.emphasized, .stronglyEmphasized])
    }

    @Test func unmatchedMarkersStayText() {
        #expect(String(AttributedString(title: "2 * 3").characters) == "2 * 3")
    }
}
