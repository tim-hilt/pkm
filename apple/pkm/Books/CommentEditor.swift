import SwiftUI

/// A Comment in a popover: shown with clickable web links, or edited, where links
/// are plain text. "Edit" switches to editing; without a Comment, "Add Comment" does.
/// The edit is saved when "Done" is pressed or the view goes; emptying the Comment
/// saves it as empty, which the owner takes as removing it.
struct CommentEditor: View {
    let comment: String?
    /// Called with the edited text when editing ends, if it changed or is empty.
    let onComment: (String) -> Void

    @State private var isEditing: Bool
    @State private var draft: String
    @FocusState private var isFocused: Bool

    /// With `startsEditing`, the editor is open from the start, for typing right away.
    init(comment: String?, startsEditing: Bool = false, onComment: @escaping (String) -> Void) {
        self.comment = comment
        self.onComment = onComment
        _isEditing = State(initialValue: startsEditing)
        _draft = State(initialValue: comment ?? "")
    }

    var body: some View {
        content.onDisappear(perform: commit)
    }

    @ViewBuilder private var content: some View {
        if isEditing {
            VStack(alignment: .trailing) {
                SwiftUI.TextEditor(text: $draft)
                    .focused($isFocused)
                    .frame(minHeight: 80, maxHeight: 200)
                    .overlay { RoundedRectangle(cornerRadius: 6).stroke(.quaternary) }
                    .onAppear { isFocused = true }
                Button("Done", action: commit)
            }
        } else if let comment {
            VStack(alignment: .leading) {
                Text(CommentLinks.attributed(comment))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
                Button("Edit") { startEditing(comment) }
            }
        } else {
            Button("Add Comment", systemImage: "comment.text.badge.plus") { startEditing("") }
        }
    }

    private func startEditing(_ comment: String) {
        draft = comment
        isEditing = true
    }

    /// Ends editing and saves the Comment if it changed. An empty one is saved too:
    /// a new page Comment has nothing to change but is to go.
    private func commit() {
        guard isEditing else { return }
        isEditing = false
        if draft != (comment ?? "") || draft.isEmpty { onComment(draft) }
    }
}
