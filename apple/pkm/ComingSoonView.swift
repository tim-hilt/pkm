import SwiftUI

/// The empty state of a Context that is not built yet.
struct ComingSoonView: View {
    let context: AppContext

    var body: some View {
        NavigationStack {
            ContentUnavailableView {
                context.label
            } description: {
                Text("Coming soon")
            }
            .toolbar {
                #if os(iOS)
                    ToolbarItem(placement: .topBarTrailing) { SettingsButton() }
                #endif
            }
        }
    }
}

#if os(iOS)
    /// The gear at the same spot of every Context's root toolbar.
    struct SettingsButton: View {
        @Environment(AppNavigation.self) private var navigation

        var body: some View {
            Button("Settings", systemImage: "gearshape") { navigation.isShowingSettings = true }
                .accessibilityIdentifier("settings")
        }
    }
#endif
