import SwiftUI

extension FocusedValues {
    /// Toggles Inline formatting on the selection of the Title or Description being edited; nil
    /// while neither is.
    var formatText: TextFormatter? {
        get { self[FormatTextKey.self] }
        set { self[FormatTextKey.self] = newValue }
    }
}

private struct FormatTextKey: FocusedValueKey {
    typealias Value = TextFormatter
}

/// Toggles Inline formatting on the selection of `field`. Equal for the same field: a new closure
/// on every update would change the scene's focused value each time, and on iPadOS 26 the menu
/// bar's update then updates the field again, without end.
struct TextFormatter: Equatable {
    let field: AnyHashable
    let apply: (Inline) -> Void

    func callAsFunction(_ kind: Inline) { apply(kind) }

    static func == (a: Self, b: Self) -> Bool { a.field == b.field }
}

/// Format menu on the Mac, and the ⌘ overlay on an iPad with a keyboard: ⌘B, ⌘I and ⌘E format
/// the selection of the field being edited (ADR-0028). It takes the place of the system's
/// rich-text Format menu, which doesn't apply to Markdown.
struct FormatCommands: Commands {
    @FocusedValue(\.formatText) private var format

    var body: some Commands {
        CommandGroup(replacing: .textFormatting) {
            Button("Bold") { format?(.strong) }
                .keyboardShortcut("b", modifiers: .command)
                .disabled(format == nil)
            Button("Italic") { format?(.emphasis) }
                .keyboardShortcut("i", modifiers: .command)
                .disabled(format == nil)
            Button("Code") { format?(.code) }
                .keyboardShortcut("e", modifiers: .command)
                .disabled(format == nil)
        }
    }
}
