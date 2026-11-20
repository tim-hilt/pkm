import SwiftUI

extension AttributedString {
    /// A Title with its Inline formatting (the core's `parseTitle`): emphasis, strong and code, and ligatures
    /// (`->` as →) when markers are removed.
    /// Markers are removed, or with `markers: .hidden` kept but made too small to see, so that the
    /// text keeps its length for an editor; `.shown` keeps them as typed.
    init(title: String, markers: Markers = .removed) {
        self.init(title)
        let spans = parseTitle(text: title)
        for span in spans {  // outer spans first, so an inner one adds to what its outer set
            guard let range = Range(span.nsRange, in: self) else { continue }
            if span.kind == .code {
                // Not the `.code` intent, which `Text` draws in the text font.
                self[range].font = .system(.body, design: .monospaced)
                continue
            }
            var intent = self[range].inlinePresentationIntent ?? []
            intent.insert(span.kind == .strong ? .stronglyEmphasized : .emphasized)
            self[range].inlinePresentationIntent = intent
        }
        guard markers != .shown else { return }
        let ranges = spans.flatMap { [$0.openingMarker, $0.closingMarker] }
        // Sequences like `->` become one glyph where the text isn't edited; no marker overlaps one.
        let glyphs = markers == .removed ? ligatures(text: title) : []
        let edits = ranges.map { ($0, String?.none) } + glyphs.map { ($0.nsRange, $0.glyph) }
        // Last first, so that changing one doesn't move the ranges before it.
        for (edit, glyph) in edits.sorted(by: { $0.0.location > $1.0.location }) {
            guard let range = Range(edit, in: self) else { continue }
            if let glyph {
                let attributes = self[range].runs.first?.attributes ?? AttributeContainer()
                replaceSubrange(range, with: AttributedString(glyph, attributes: attributes))
            } else if markers == .removed {
                removeSubrange(range)
            } else {
                self[range].font = .system(size: 0.01)
            }
        }
    }

    enum Markers { case shown, hidden, removed }
}

extension Ligature {
    fileprivate var nsRange: NSRange { NSRange(location: Int(start), length: Int(end - start)) }
}

extension InlineSpan {
    fileprivate var nsRange: NSRange { NSRange(location: Int(start), length: Int(end - start)) }
    fileprivate var openingMarker: NSRange { NSRange(location: Int(start), length: Int(markerLen)) }
    fileprivate var closingMarker: NSRange { NSRange(location: Int(end - markerLen), length: Int(markerLen)) }
}
