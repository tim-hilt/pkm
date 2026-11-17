import SwiftUI

/// The empty state of a Context that is not built yet.
struct ComingSoonView: View {
    let context: AppContext

    var body: some View {
        ContentUnavailableView {
            context.label
        } description: {
            Text("Coming soon")
        }
    }
}
