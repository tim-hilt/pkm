import SwiftUI

extension AttributedString {
    /// A Search snippet as typed, with the match in bold; `matchedStart` up to `matchedEnd` count
    /// UTF-16 units of `snippet`.
    init(snippet: String, matchedStart: UInt32, matchedEnd: UInt32) {
        self.init(snippet)
        let units = snippet.utf16
        if let from = units.index(units.startIndex, offsetBy: Int(matchedStart), limitedBy: units.endIndex),
            let to = units.index(units.startIndex, offsetBy: Int(matchedEnd), limitedBy: units.endIndex),
            let range = Range(NSRange(from..<to, in: snippet), in: self)
        {
            self[range].font = .subheadline.bold()
        }
    }
}
