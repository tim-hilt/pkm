import SwiftUI

/// Stands in for the Reader until it arrives: the open Book's Title and a way back.
struct BookStubView: View {
    let book: Book
    let onBack: () -> Void

    var body: some View {
        NavigationStack {
            ContentUnavailableView {
                Label(book.title, systemImage: "book")
            } description: {
                Text("The Reader is coming soon")
            }
            .toolbar {
                ToolbarItem(placement: .navigation) {
                    Button("Back", systemImage: "chevron.backward", action: onBack)
                        .accessibilityIdentifier("back")
                }
            }
        }
    }
}
