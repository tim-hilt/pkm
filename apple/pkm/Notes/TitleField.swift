import SwiftUI

/// The Title of a Note: one line with Inline formatting, raw while focused and formatted
/// otherwise, and ⌘B, ⌘I and ⌘E on the selection. Unlike a Task's Title it has no Quick Add.
struct TitleField: View {
    @Binding var text: String
    /// Takes the keyboard focus when it appears.
    var autofocus = false
    @FocusState private var isFocused: Bool
    @State private var selection: TextSelection?

    var body: some View {
        TextField("Title", text: $text, selection: $selection)
            .font(.title3)
            .focused($isFocused)
            .foregroundStyle(isFocused ? .primary : Color.clear)
            .overlay(alignment: .leading) {
                if !isFocused && !text.isEmpty {
                    Text(AttributedString(title: text)).font(.title3).allowsHitTesting(false)
                }
            }
            .focusedSceneValue(\.formatText, isFocused ? { format($0) } : nil)
            .onAppear { isFocused = autofocus }
            .accessibilityIdentifier("note-title")
    }

    /// Toggles Inline formatting on the selection, which stays selected.
    private func format(_ kind: Inline) {
        var range = text.endIndex..<text.endIndex
        if case .selection(let selected)? = selection?.indices { range = selected }
        let selected = NSRange(range, in: text)
        let edit = toggleInline(
            text: text, start: UInt32(selected.location), end: UInt32(selected.location + selected.length),
            kind: kind)
        let replaced = NSRange(location: Int(edit.start), length: Int(edit.end - edit.start))
        let formatted = (text as NSString).replacingCharacters(in: replaced, with: edit.replacement)
        text = formatted
        let after = NSRange(location: Int(edit.selectionStart), length: Int(edit.selectionEnd - edit.selectionStart))
        if let after = Range(after, in: formatted) { selection = TextSelection(range: after) }
    }
}
