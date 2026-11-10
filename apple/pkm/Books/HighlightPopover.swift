import SwiftUI

/// What a Highlight's popover offers: its Comment (`CommentEditor`), its color among the
/// Highlight Colors in a row of five that scrolls, the current one checked (an
/// extra circle if it is no longer one), and removing it without confirmation.
struct HighlightPopover: View {
    let colors: [HighlightColor]
    let color: RGB
    let comment: String?
    let onColor: (RGB) -> Void
    /// Called with the edited Comment; empty means the Comment is removed.
    let onComment: (String) -> Void
    let onRemove: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            CommentEditor(comment: comment, onComment: onComment)
            HighlightColorGrid(
                colors: colors.including(color), selected: color, scrollingRow: 5, onSelect: { onColor($0.tint) })
            Button("Remove Highlight", systemImage: "trash", role: .destructive, action: onRemove)
        }
        .padding()
        // A popover is as big as its content's ideal size; at no width, the grid
        // is a single column and a Comment a single line.
        .frame(minWidth: 260, idealWidth: 280)
        .presentationDetents([.medium, .large])
    }
}

/// What a page Comment's popover offers: its Comment (`CommentEditor`), and removing it
/// without confirmation. A new page Comment opens in editing.
struct PageCommentPopover: View {
    let text: String
    let startsEditing: Bool
    /// Called with the edited Comment; empty means the page Comment is removed.
    let onText: (String) -> Void
    let onRemove: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            CommentEditor(comment: text.isEmpty ? nil : text, startsEditing: startsEditing, onComment: onText)
            Button("Remove Comment", systemImage: "trash", role: .destructive, action: onRemove)
        }
        .padding()
        .frame(minWidth: 260, idealWidth: 280)
        .presentationDetents([.medium, .large])
    }
}
