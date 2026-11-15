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
        }
    }
}
