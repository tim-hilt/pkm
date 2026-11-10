import SwiftUI

/// A Book's File opened for the Reader, in the book view of its Format.
enum OpenedBook {
    case pdf(StyledDocument)
    case epub(EpubBook)
}

/// Everything the Reader shows of a Book when it opens, read before it is
/// pushed, so that it shows complete at once: bar, pages and Markup.
struct LoadedBook {
    /// The `Book` it was read from.
    let id: String
    let file: OpenedBook
    /// Empty if the Book has none.
    let contents: TableOfContents
    let highlights: [Highlight]
    let pageComments: [PageComment]

    /// Reads `book`: its File, drawn per `appearance` from the start, Table
    /// of Contents and Markup. The EPUB is opened and the Table of Contents built
    /// off the main thread. Nil if the File can't be read.
    @MainActor
    static func load(
        _ book: Book, in library: LibraryModel, appearance: PageAppearance
    ) async -> LoadedBook? {
        async let markup = library.markup(of: book)
        guard let (file, contents) = await open(book, in: library, appearance: appearance) else { return nil }
        let (highlights, pageComments) = await markup
        return LoadedBook(
            id: book.id, file: file, contents: contents, highlights: highlights, pageComments: pageComments)
    }

    @MainActor
    private static func open(
        _ book: Book, in library: LibraryModel, appearance: PageAppearance
    ) async -> (OpenedBook, TableOfContents)? {
        switch book.format {
        case .pdf:
            // Styled before PDFKit sees it, so it never starts drawing in Light Pages.
            guard let pdf = StyledDocument(url: book.file) else { return nil }
            pdf.appearance = appearance
            // Nothing else uses the PDF until the Reader has it.
            nonisolated(unsafe) let unshared = pdf
            let contents = await Task.detached { TableOfContents(unshared) }.value
            return (.pdf(pdf), contents)
        case .epub:
            guard let epub = await library.epub(of: book) else { return nil }
            let contents = await Task.detached { epub.tableOfContents() }.value
            return (.epub(epub), contents)
        }
    }
}

/// Everything the Reader hands a document view, whatever its Format. It reports
/// through `handle` and `callbacks`; see `ReaderBookView`.
struct ReaderInputs {
    /// The Page Style to show.
    let appearance: PageAppearance
    let highlights: [Highlight]
    let pageComments: [PageComment]
    let pageCommentColor: RGB
    let handle: ReaderHandle
    let results: SearchResults
    /// Where to open; nil opens at the start.
    let place: Place?
    /// The Table of Contents, which names the Back place.
    let contents: TableOfContents
    /// The text size of EPUBs; see `TextSize`.
    let textSize: CGFloat
    let callbacks: ReaderCallbacks
    /// Called when the reader asks for the list of matches in the find bar (iOS only).
    let onShowResults: () -> Void
}

/// The view that shows an `OpenedBook`, picking the book view of its Format.
struct ReaderContent: View {
    let book: OpenedBook
    let inputs: ReaderInputs

    var body: some View {
        switch book {
        case .pdf(let pdf):
            ReaderPDFContent(document: pdf, inputs: inputs)
                #if os(iOS)
                    // Pages run edge to edge in landscape, across the Dynamic Island.
                    .ignoresSafeArea(.container, edges: .horizontal)
                #endif
        case .epub(let book): ReaderEPUBContent(book: book, inputs: inputs)
        }
    }
}
