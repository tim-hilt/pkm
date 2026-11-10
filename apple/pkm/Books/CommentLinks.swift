import Foundation

/// How a Comment is shown: its text with the web links in it clickable. Links are
/// found when it is shown and never stored.
enum CommentLinks {
    private static let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)

    /// `comment` as it is, with `http` and `https` links as links.
    static func attributed(_ comment: String) -> AttributedString {
        var shown = AttributedString(comment)
        let matches = detector?.matches(in: comment, range: NSRange(comment.startIndex..., in: comment)) ?? []
        for match in matches {
            guard let url = match.url, ["http", "https"].contains(url.scheme?.lowercased()),
                let range = Range(match.range, in: shown)
            else { continue }
            shown[range].link = url
        }
        return shown
    }
}
