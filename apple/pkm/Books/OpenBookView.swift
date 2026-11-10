import SwiftUI

/// The Reader of the open Book, filling the window. Reads the Book first and
/// shows the Reader complete; a Book whose File can't be read goes back to the
/// Gallery. The Reader, with a fresh state per Book, Goes to Gallery by `onGoToGallery`.
struct OpenBookView: View {
    let book: Book
    let onGoToGallery: () -> Void
    @Environment(LibraryModel.self) private var library
    @Environment(ReaderSettings.self) private var settings
    @Environment(\.colorScheme) private var colorScheme
    @State private var loaded: LoadedBook?

    var body: some View {
        NavigationStack {
            if let loaded {
                ReaderView(book: book, loaded: loaded, onGoToGallery: onGoToGallery)
            } else {
                ProgressView()
                    .toolbar {
                        ToolbarItem(placement: .navigation) {
                            Button("Books", systemImage: "chevron.backward", action: onGoToGallery)
                                .accessibilityIdentifier("back")
                        }
                    }
            }
        }
        .task(id: book.id) {
            let read = await LoadedBook.load(
                book, in: library, appearance: settings.pageAppearance(systemScheme: colorScheme))
            guard !Task.isCancelled else { return }
            if read == nil { onGoToGallery() }
            loaded = read
        }
    }
}
