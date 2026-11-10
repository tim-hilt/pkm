import Foundation
import Testing

@testable import pkm

@Suite struct CommentLinksTests {
    private func links(in comment: String) -> [(text: String, url: URL)] {
        let shown = CommentLinks.attributed(comment)
        return shown.runs.compactMap { run in
            run.link.map { (String(shown[run.range].characters), $0) }
        }
    }

    @Test func webLinksInANoteAreClickable() {
        let found = links(in: "see https://example.com/a?b=c and http://example.org.")

        #expect(found.map(\.url.absoluteString) == ["https://example.com/a?b=c", "http://example.org"])
        #expect(found.map(\.text) == ["https://example.com/a?b=c", "http://example.org"])
    }

    @Test func otherLinkTypesStayPlainText() {
        #expect(links(in: "mail me@example.com or ftp://example.com/file or tel:+4912345678").isEmpty)
    }

    @Test func theCommentTextIsShownUnchanged() {
        let comment = "a https://example.com b\nsecond line"
        #expect(String(CommentLinks.attributed(comment).characters) == comment)
    }

    @Test func aCommentWithoutLinksHasNone() {
        #expect(links(in: "just a thought").isEmpty)
    }
}
