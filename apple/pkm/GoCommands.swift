import SwiftUI

/// The Go menu: ⌘1–⌘5 switch Contexts, except while an Item is open.
struct GoCommands: Commands {
    let navigation: AppNavigation

    var body: some Commands {
        CommandMenu("Go") {
            ForEach(AppContext.allCases) { context in
                Button(String(localized: context.title)) { navigation.selected = context }
                    .keyboardShortcut(KeyEquivalent(Character("\(context.shortcutDigit)")))
                    .disabled(!navigation.canSwitchContext)
            }
        }
    }
}
