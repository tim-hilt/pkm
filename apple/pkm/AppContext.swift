import SwiftUI

/// One of the app's five Contexts, in the fixed order of the tabs (named so
/// that it does not shadow SwiftUI's `Context`).
enum AppContext: String, CaseIterable, Identifiable {
    case tasks, notes, drawings, bookmarks, books

    var id: String { rawValue }

    var title: LocalizedStringResource {
        switch self {
        case .tasks: "Tasks"
        case .notes: "Notes"
        case .drawings: "Drawings"
        case .bookmarks: "Bookmarks"
        case .books: "Books"
        }
    }

    var symbol: String {
        switch self {
        case .tasks: "checklist"
        case .notes: "note.text"
        case .drawings: "scribble.variable"
        case .bookmarks: "bookmark"
        case .books: "books.vertical"
        }
    }

    /// The digit that, with ⌘, switches to the Context: 1 for the first.
    var shortcutDigit: Int { Self.allCases.firstIndex(of: self)! + 1 }
}
