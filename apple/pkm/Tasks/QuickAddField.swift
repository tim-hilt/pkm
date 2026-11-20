import SwiftUI

/// The Title field of a Task: Quick Add's text, with its Shortcuts highlighted, and the Tags
/// to suggest below it while the cursor is in an `@` word. It only draws and forwards what is typed,
/// the cursor, Return, picks and Ignores; every rule lives in the core's `TaskQuickAdd`.
struct QuickAddField: View {
    @Environment(TasksModel.self) private var model
    let quickAdd: TaskQuickAdd
    /// Quick Add's Title (the text without Shortcuts), kept up to date.
    @Binding var title: String
    /// Changes when the field must redraw because Quick Add was changed from outside (replaced, or
    /// its Priority Ignored).
    var revision = 0
    /// A change of `revision` also takes the focus back, e.g. after Add, when it may be elsewhere.
    var refocusOnRevision = false
    /// Tags the Task already has, which the list doesn't suggest.
    var taskTags: [String] = []
    /// Takes the keyboard focus when it appears.
    var autofocus = true
    /// Return was pressed.
    let onSubmit: () -> Void
    /// The field lost focus. Not while the suggestion list is open: picking from it may take focus
    /// for a moment, and the `@` word is not finished.
    var onBlur: () -> Void = {}
    @State private var text = AttributedString()
    @State private var selection = AttributedTextSelection()
    /// The plain text the editor shows, to tell typing from our own re-drawing of highlights.
    @State private var shown = ""
    @State private var highlighted: [QuickShortcut] = []
    @FocusState private var isFocused: Bool
    @State private var query: TagQuery?
    /// The suggestion the arrow keys are on.
    @State private var selected = 0
    /// Escape closed the list for this query.
    @State private var isClosed = false

    /// What the list offers: matching Tags, then Create for a new valid one; empty while closed.
    private var suggestions: [Suggestion] {
        guard let query, !isClosed else { return [] }
        let found = model.suggestTags(query.query, leaveOut: query.leaveOut + taskTags)
        return found.matches.map { Suggestion(tag: $0, isNew: false) }
            + (found.create ? [Suggestion(tag: query.query, isNew: true)] : [])
    }

    private struct Suggestion: Identifiable {
        let tag: String
        let isNew: Bool
        var id: String { tag }
    }

    var body: some View {
        // The list grows the row instead of floating: a Form clips a row's overlay (iOS) and draws
        // the next row over it (macOS).
        VStack(alignment: .leading, spacing: 8) {
            editor
            if !suggestions.isEmpty {
                suggestionList
            } else {
                #if os(iOS)
                    if isFocused { tagButton }
                #endif
            }
        }
    }

    #if os(iOS)
        /// Types "@" for a Tag Shortcut. It sits below the editor, where a keyboard bar would cover text.
        private var tagButton: some View {
            Button {
                text.replaceSelection(&selection, withCharacters: "@")
            } label: {
                Label("Tag", systemImage: "tag")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
    #endif

    private var suggestionList: some View {
        VStack(spacing: 2) {
            ForEach(Array(suggestions.enumerated()), id: \.element.id) { index, suggestion in
                Button {
                    pick(suggestion)
                } label: {
                    Label {
                        Text(suggestion.isNew ? "Create Tag '\(suggestion.tag)'" : suggestion.tag)
                    } icon: {
                        Image(systemName: "tag")
                    }
                    .foregroundStyle(model.tagEntry(suggestion.tag).style(default: .primary))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .background(
                        index == selected ? Color.accentColor.opacity(0.2) : .clear, in: .rect(cornerRadius: 10)
                    )
                    .padding(.horizontal, -8)  // the highlight's margin, without moving the label
                }
                .buttonStyle(.plain)
            }
        }
    }

    private var editor: some View {
        SwiftUI.TextEditor(text: $text, selection: $selection)
            .scrollContentBackground(.hidden)
            .scrollDisabled(true)
            .padding(.horizontal, -5)  // the editor's own inset, to line up with the other fields
            .focused($isFocused)
            #if os(iOS)
                .textInputAutocapitalization(.sentences)
                .autocorrectionDisabled(false)
            #endif
            .onAppear {
                draw()
                isFocused = autofocus
            }
            .onChange(of: isFocused) {
                draw()
                if !isFocused && suggestions.isEmpty { onBlur() }
            }
            .onChange(of: revision) {
                if refocusOnRevision { isFocused = true }
                draw(placingCursor: true)
            }
            .overlay(alignment: .topLeading) {
                if text.characters.isEmpty {
                    Text("Title").foregroundStyle(.tertiary).allowsHitTesting(false)
                }
            }
            .accessibilityRepresentation { shortcutElements }
            .onChange(of: text) { typed() }
            .onChange(of: selection) { cursorMoved() }
            .focusedSceneValue(\.formatText, formatting)
            .onKeyPress(.downArrow) { move(by: 1) }
            .onKeyPress(.upArrow) { move(by: -1) }
            .onKeyPress(.tab, phases: .down) { press in
                if pickSelected() { return .handled }
                #if os(macOS)
                    // The editor would insert a tab; Tab moves on to the next field, Shift-Tab back.
                    if press.modifiers.isSubset(of: .shift) {
                        let window = NSApp.keyWindow
                        if press.modifiers.contains(.shift) {
                            window?.selectPreviousKeyView(nil)
                        } else {
                            window?.selectNextKeyView(nil)
                        }
                        return .handled
                    }
                #endif
                return .ignored
            }
            .onKeyPress(.escape) {
                guard !suggestions.isEmpty else { return .ignored }
                isClosed = true
                return .handled
            }
    }

    /// The field for VoiceOver: the editor, then each Shortcut with its meaning and an Ignore action.
    private var shortcutElements: some View {
        VStack {
            SwiftUI.TextEditor(text: $text, selection: $selection)
            ForEach(highlighted, id: \.start) { shortcut in
                Text(Self.label(of: shortcut, in: shown))
                    .accessibilityAction(named: "Ignore") { ignore(at: shortcut.start) }
            }
        }
    }

    /// "p1, Priority 1" or "@bug, Tag bug".
    static func label(of shortcut: QuickShortcut, in text: String) -> LocalizedStringResource {
        let word = (text as NSString).substring(
            with: NSRange(location: Int(shortcut.start), length: Int(shortcut.end - shortcut.start)))
        switch shortcut.meaning {
        case .priority(let p): return "\(word), Priority \(Int(p))"
        case .tag(let tag): return "\(word), Tag \(tag)"
        }
    }

    private func ignore(at position: UInt32) {
        quickAdd.ignore(at: position)
        draw()
    }

    private func move(by step: Int) -> KeyPress.Result {
        let count = suggestions.count
        guard count > 0 else { return .ignored }
        selected = (selected + step + count) % count
        return .handled
    }

    /// Picks the suggestion the arrow keys are on; false if the list is closed.
    private func pickSelected() -> Bool {
        let list = suggestions
        guard list.indices.contains(selected) else { return false }
        pick(list[selected])
        return true
    }

    private func pick(_ suggestion: Suggestion) {
        quickAdd.pickTag(tag: suggestion.tag)
        draw()
        isFocused = true
    }

    /// Hands what changed in the editor to Quick Add; Return submits instead of adding a line.
    private func typed() {
        let now = String(text.characters)
        guard now != shown else { return }
        let (range, inserted) = Self.edit(from: shown, to: now)
        if inserted == "\n" || inserted == "\r" {
            // While the list is open Return picks instead of adding.
            if !pickSelected() {
                draw()
                onSubmit()
            }
            return
        }
        quickAdd.replace(start: range.lowerBound, end: range.upperBound, with: inserted)
        draw()
    }

    /// What formats the selection (see `FormatCommands`) while the field has the focus.
    private var formatting: ((Inline) -> Void)? {
        guard isFocused else { return nil }
        return { format($0) }
    }

    /// Toggles Inline formatting on the selection, which stays selected.
    private func format(_ kind: Inline) {
        let (start, end) = Self.offsets(of: selection, in: text)
        let edit = toggleInline(text: quickAdd.text(), start: start, end: end, kind: kind)
        quickAdd.replace(start: edit.start, end: edit.end, with: edit.replacement)
        quickAdd.moveCursor(at: edit.selectionEnd)
        draw()
        let range = NSRange(location: Int(edit.selectionStart), length: Int(edit.selectionEnd - edit.selectionStart))
        if let range = Range(range, in: text) { selection = AttributedTextSelection(range: range) }
    }

    /// A jump of the cursor into a highlight (a tap or click, unlike typing or the arrow keys, which
    /// move by one unit) Ignores that Shortcut.
    private func cursorMoved() {
        // Typing or pasting moved it; `typed` hands that over, while the highlights still match the old text.
        guard String(text.characters) == shown else { return }
        let at = Self.cursor(of: selection, in: text)
        if at.distance(to: quickAdd.cursor()).magnitude > 1,
            let hit = highlighted.first(where: { $0.start < at && at < $0.end })
        {
            quickAdd.moveCursor(at: at)
            ignore(at: hit.start)
            return
        }
        quickAdd.moveCursor(at: at)
        if quickAdd.highlights() != highlighted || quickAdd.tagQuery() != query { draw() }
    }

    /// Shows Quick Add's text with its highlights, keeping the cursor where Quick Add has it.
    private func draw(placingCursor: Bool = false) {
        let plain = quickAdd.text()
        title = quickAdd.title()
        highlighted = quickAdd.highlights()
        // Raw markers while focused; unfocused, formatted with the markers too small to see.
        var drawn = AttributedString(title: plain, markers: isFocused ? .shown : .hidden)
        for shortcut in highlighted {
            let range = NSRange(location: Int(shortcut.start), length: Int(shortcut.end - shortcut.start))
            if let range = Range(range, in: drawn) {
                drawn[range].foregroundColor = color(of: shortcut)
                drawn[range].inlinePresentationIntent = .stronglyEmphasized
            }
        }
        let moved = plain != String(text.characters)
        shown = plain
        text = drawn
        if moved || placingCursor {
            let cursor = NSRange(location: Int(quickAdd.cursor()), length: 0)
            selection = AttributedTextSelection(insertionPoint: Range(cursor, in: drawn)?.lowerBound ?? drawn.endIndex)
        }
        let now = quickAdd.tagQuery()
        if now != query {
            query = now
            selected = 0
            isClosed = false
        }
    }

    /// A Tag Shortcut in its Tag's color; the accent color for Default and for Priorities.
    private func color(of shortcut: QuickShortcut) -> Color {
        guard case .tag(let tag) = shortcut.meaning else { return .accentColor }
        return model.tagEntry(tag).color.flatMap(Color.init(tagColor:)) ?? .accentColor
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

    /// The UTF-16 start and end of the selection; both the caret's position without a range.
    private static func offsets(of selection: AttributedTextSelection, in text: AttributedString) -> (
        start: UInt32, end: UInt32
    ) {
        let range: Range<AttributedString.Index>
        switch selection.indices(in: text) {
        case .insertionPoint(let point): range = point..<point
        case .ranges(let ranges):
            let all = ranges.ranges
            range = (all.first?.lowerBound ?? text.endIndex)..<(all.last?.upperBound ?? text.endIndex)
        }
        let at = NSRange(range, in: text)
        return (UInt32(at.location), UInt32(at.location + at.length))
    }

    /// The UTF-16 position of the caret, or of the end of a selection.
    private static func cursor(of selection: AttributedTextSelection, in text: AttributedString) -> UInt32 {
        offsets(of: selection, in: text).end
    }
}
