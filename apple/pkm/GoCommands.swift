import SwiftUI

/// The Go menu: ⌘1–⌘5 switch Contexts, except while an Item is open; with a
/// Book open, Back (a Jump) and the Table of Contents.
struct GoCommands: Commands {
    let navigation: AppNavigation
    #if os(macOS)
        @FocusedValue(\.reader) private var reader
        @FocusedValue(\.showTableOfContents) private var showTableOfContents
        @FocusedValue(\.backTarget) private var backTarget
    #endif

    var body: some Commands {
        CommandMenu("Go") {
            ForEach(AppContext.allCases) { context in
                Button(String(localized: context.title)) { navigation.selected = context }
                    .keyboardShortcut(KeyEquivalent(Character("\(context.shortcutDigit)")))
                    .disabled(!navigation.canSwitchContext)
            }
            #if os(macOS)
                if reader != nil {
                    Divider()
                    Button(backTarget.map { "Back to \($0.name)" } ?? "Back") { reader?.view?.goBack() }
                        .disabled(backTarget == nil)
                    Button("Table of Contents") { showTableOfContents?.wrappedValue = true }
                        .keyboardShortcut("t", modifiers: [.option, .command])
                        .disabled(showTableOfContents == nil)
                }
            #endif
        }
    }
}
