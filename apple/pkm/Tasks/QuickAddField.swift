import SwiftUI

/// The Title field of a new Task: Quick Add's text, with its Shortcuts highlighted. It only draws
/// and forwards what is typed, the cursor and Return; every rule lives in the core's `TaskQuickAdd`.
struct QuickAddField: View {
    let quickAdd: TaskQuickAdd
    /// Quick Add's Title (the text without Shortcuts), kept up to date.
    @Binding var title: String
    /// Return was pressed.
    let onSubmit: () -> Void
    @State private var text = AttributedString()
    @State private var selection = AttributedTextSelection()
    /// The plain text the editor shows, to tell typing from our own re-drawing of highlights.
    @State private var shown = ""
    @State private var highlighted: [QuickShortcut] = []
    @FocusState private var isFocused: Bool

    var body: some View {
        SwiftUI.TextEditor(text: $text, selection: $selection)
            .scrollContentBackground(.hidden)
            .scrollDisabled(true)
            .padding(.horizontal, -5)  // the editor's own inset, to line up with the other fields
            .focused($isFocused)
            .onAppear { isFocused = true }
            .overlay(alignment: .topLeading) {
                if text.characters.isEmpty {
                    Text("Title").foregroundStyle(.tertiary).allowsHitTesting(false)
                }
            }
            .onChange(of: text) { typed() }
            .onChange(of: selection) { cursorMoved() }
    }

    /// Hands what changed in the editor to Quick Add; Return submits instead of adding a line.
    private func typed() {
        let now = String(text.characters)
        guard now != shown else { return }
        let (range, inserted) = Self.edit(from: shown, to: now)
        if inserted == "\n" || inserted == "\r" {
            draw()
            onSubmit()
            return
        }
        quickAdd.replace(start: range.lowerBound, end: range.upperBound, with: inserted)
        draw()
    }

    private func cursorMoved() {
        quickAdd.moveCursor(at: Self.cursor(of: selection, in: text))
        if quickAdd.highlights() != highlighted { draw() }
    }

    /// Shows Quick Add's text with its highlights, keeping the cursor where Quick Add has it.
    private func draw() {
        let plain = quickAdd.text()
        title = quickAdd.title()
        highlighted = quickAdd.highlights()
        var drawn = AttributedString(plain)
        for shortcut in highlighted {
            let range = NSRange(location: Int(shortcut.start), length: Int(shortcut.end - shortcut.start))
            if let range = Range(range, in: drawn) { drawn[range].backgroundColor = Color.accentColor.opacity(0.25) }
        }
        let moved = plain != String(text.characters)
        shown = plain
        text = drawn
        if moved { selection = AttributedTextSelection(insertionPoint: drawn.endIndex) }
    }

    /// The UTF-16 range of `old` that `new` replaced, and what replaced it: all but their common
    /// start and end.
    static func edit(from old: String, to new: String) -> (range: Range<UInt32>, inserted: String) {
        let (old16, new16) = (Array(old.utf16), Array(new.utf16))
        let limit = min(old16.count, new16.count)
        var head = 0
        while head < limit, old16[head] == new16[head] { head += 1 }
        var tail = 0
        while tail < limit - head, old16[old16.count - 1 - tail] == new16[new16.count - 1 - tail] { tail += 1 }
        let inserted = String(decoding: new16[head..<(new16.count - tail)], as: UTF16.self)
        return (UInt32(head)..<UInt32(old16.count - tail), inserted)
    }

    /// The UTF-16 position of the caret, or of the end of a selection.
    private static func cursor(of selection: AttributedTextSelection, in text: AttributedString) -> UInt32 {
        let index: AttributedString.Index
        switch selection.indices(in: text) {
        case .insertionPoint(let point): index = point
        case .ranges(let ranges): index = ranges.ranges.last?.upperBound ?? text.endIndex
        }
        return UInt32(NSRange(index..<index, in: text).location)
    }
}
