import Foundation

/// What a tap on the Article can land on.
enum MarkupTarget: Equatable {
    case highlight(String)
    case comment(String)
}

extension ArticleMarkup {
    /// The Highlight over the UTF-16 `offset`, the newest if several, else the Comment on that
    /// character of a text `length` long.
    func target(at offset: Int, length: Int) -> MarkupTarget? {
        if let highlight = highlights.last(where: { Int($0.start) <= offset && offset < Int($0.end) }) {
            return .highlight(highlight.id)
        }
        return pageComments.last { Self.character(of: $0, length: length) == offset }.map { .comment($0.id) }
    }

    /// The UTF-16 range a target occupies: a Highlight's passage, a Comment's character.
    func range(of target: MarkupTarget, length: Int) -> NSRange? {
        switch target {
        case .highlight(let id):
            guard let h = highlights.first(where: { $0.id == id }) else { return nil }
            return NSRange(location: Int(h.start), length: Int(h.end - h.start))
        case .comment(let id):
            guard let c = pageComments.first(where: { $0.id == id }), length > 0 else { return nil }
            return NSRange(location: Self.character(of: c, length: length), length: 1)
        }
    }

    /// The character a Comment marks: the one at its spot, at the end of the text the last.
    private static func character(of comment: ArticleComment, length: Int) -> Int {
        max(0, min(Int(comment.offset), length - 1))
    }
}
