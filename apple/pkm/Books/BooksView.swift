import SwiftUI

/// The Books Context's first screen, the Gallery: the Library as a grid of
/// Covers after a "+" cell, sorted by Date Imported, Title or Recently Read and narrowed by
/// Library Search. Import by the "+" cell, File > Import… or dropping Files
/// onto the grid; a single File opens its Book, new or already in the Library.
/// A cell's context menu (long-press, right-click) shows the Book's
/// `BookInfo`; it or a VoiceOver action offers "Delete", after confirmation.
///
/// With a keyboard (Mac, iPad), arrow keys move a highlight through the grid,
/// including the "+" cell; nothing is highlighted until the first press.
/// Return opens the highlighted cell, ⌘⌫ asks to Delete it, and ↓ in the
/// search field highlights the first match.
struct BooksView: View {
    @Environment(LibraryModel.self) private var library
    @State private var isImporting = false
    @State private var isDropTargeted = false
    @State private var rejection: ImportAlert?
    @State private var deleting: Book?
    @State private var scroll = ScrollActivity()
    @State private var columnCount = 2
    /// The keyboard highlight; nil until the first arrow press.
    @State private var highlighted: GalleryItem?
    @FocusState private var isGridFocused: Bool
    @FocusState private var isSearchFocused: Bool
    /// Library Search; empty when not searching.
    @State private var searchText = ""

    /// The Books shown, in grid order after the "+" cell.
    private var visibleBooks: [Book] { library.visibleBooks(matching: searchText) }
    /// Every cell in grid order.
    private var items: [GalleryItem] { [.importCell] + visibleBooks.map { .book($0.id) } }

    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                ScrollView { grid }
                    .onDrop(of: ImportableTypes.all, isTargeted: $isDropTargeted, perform: drop)
                    .overlay {
                        if isDropTargeted { Rectangle().strokeBorder(.tint, lineWidth: 4).allowsHitTesting(false) }
                    }
                    .onGeometryChange(for: Int.self) {
                        GalleryLayout.columnCount(forWidth: $0.size.width)
                    } action: {
                        columnCount = $0
                    }
                    .focusable()
                    .focused($isGridFocused)
                    .focusEffectDisabled()
                    .onAppear { isGridFocused = true }
                    .onChange(of: highlighted) { _, item in
                        if let item { proxy.scrollTo(item) }
                    }
            }
            .onScrollPhaseChange { _, phase in scroll.scrollingChanged(isScrolling: phase != .idle) }
            .environment(scroll)
            .searchable(text: $searchText)
            .searchFocused($isSearchFocused)
            .navigationTitle("Books")
            .toolbar {
                ToolbarItem { SortMenu(sort: Bindable(library).sort) }
                #if os(iOS)
                    ToolbarItem(placement: .topBarTrailing) { SettingsButton() }
                #endif
            }
        }
        .onKeyPress(phases: [.down, .repeat]) { press in
            guard let arrow = GridHighlight.Arrow(press.key) else { return .ignored }
            return handleArrow(arrow) ? .handled : .ignored
        }
        .onKeyPress(keys: [.return, .delete], phases: .down) { press in
            handleAction(press) ? .handled : .ignored
        }
        .disabled(library.runningImport != nil)
        .focusedSceneValue(\.isImporting, library.runningImport == nil ? $isImporting : nil)
        .overlay {
            if let running = library.runningImport { ImportProgressModal(running: running) }
        }
        .fileImporter(
            isPresented: $isImporting, allowedContentTypes: ImportableTypes.all, allowsMultipleSelection: true
        ) { result in
            if case .success(let urls) = result { Task { await importFiles(urls) } }
        }
        .alert(rejection?.title ?? "", isPresented: Binding(isPresenting: $rejection), presenting: rejection) { _ in
        } message: { alert in
            Text(alert.message)
        }
    }

    private func isDeleting(_ book: Book) -> Binding<Bool> {
        Binding(get: { deleting?.id == book.id }, set: { if !$0 { deleting = nil } })
    }

    /// The cells, "+" first, and "No Results" when a search matches no Book.
    private var grid: some View {
        let visibleBooks = visibleBooks
        return VStack(alignment: .leading, spacing: GalleryLayout.spacing) {
            LazyVGrid(
                columns: Array(
                    repeating: GridItem(.flexible(), spacing: GalleryLayout.spacing, alignment: .top),
                    count: columnCount),
                spacing: GalleryLayout.spacing
            ) {
                Button {
                    open(.importCell)
                } label: {
                    ImportCell()
                }
                .galleryHighlight(highlighted == .importCell)
                .id(GalleryItem.importCell)
                .accessibilityLabel("Import…")
                .accessibilityIdentifier("import")
                ForEach(visibleBooks) { book in
                    Button {
                        open(.book(book.id))
                    } label: {
                        BookCell(book: book)
                    }
                    .galleryHighlight(highlighted == .book(book.id))
                    .id(GalleryItem.book(book.id))
                    .bookContextMenu(book) { deleting = book }
                    .accessibilityLabel(book.accessibilityLabel)
                    .accessibilityIdentifier("book-\(book.id)")
                    .accessibilityAction(named: "Delete") { deleting = book }
                    // On the cell, so that the dialog's popover points at it.
                    .confirmationDialog("Delete", isPresented: isDeleting(book)) {
                        Button("Delete", role: .destructive) { Task { await library.delete(book) } }
                            .accessibilityIdentifier("delete-confirm")
                    }
                }
            }
            .buttonStyle(.plain)
            // Animates a Book to its new place when the order changes.
            .animation(.default, value: visibleBooks.map(\.id))
            if visibleBooks.isEmpty && !searchText.isEmpty {
                Text("No Results for “\(searchText)”").foregroundStyle(.secondary)
            }
        }
        .padding(GalleryLayout.spacing)
    }

    /// Opens a Book, or starts an Import for the "+" cell.
    private func open(_ item: GalleryItem) {
        switch item {
        case .importCell: isImporting = true
        case .book(let id): library.openBook(id: id)
        }
    }

    /// Moves the highlight; from the search field, ↓ highlights the first
    /// match and hands the keyboard to the grid. False if the key isn't ours.
    private func handleArrow(_ arrow: GridHighlight.Arrow) -> Bool {
        if isSearchFocused {
            guard arrow == .down, let first = visibleBooks.first else { return false }
            highlighted = .book(first.id)
            isGridFocused = true
            return true
        }
        let grid = GridHighlight(columns: columnCount, cellCount: items.count)
        let current = highlighted.flatMap(items.firstIndex(of:))
        highlighted = items[grid.moved(from: current, arrow)]
        return true
    }

    /// Return opens the highlighted cell; ⌘⌫ asks to Delete its Book.
    private func handleAction(_ press: KeyPress) -> Bool {
        guard !isSearchFocused, let highlighted else { return false }
        switch press.key {
        case .return:
            open(highlighted)
        case .delete where press.modifiers == .command:
            guard case .book(let id) = highlighted else { return false }
            deleting = visibleBooks.first { $0.id == id }
        default:
            return false
        }
        return true
    }

    /// Imports the PDFs and EPUBs among the dropped `providers`. False if there are none.
    private func drop(_ providers: [NSItemProvider]) -> Bool {
        guard DroppedFiles.accepts(providers) else { return false }
        Task {
            let dropped = DroppedFiles()
            let urls = await dropped.receive(providers)
            if !urls.isEmpty { await importFiles(urls) }
            dropped.removeCopies()
        }
        return true
    }

    private func importFiles(_ urls: [URL]) async {
        switch await library.importFiles(urls) {
        case .open(let book, let problems):
            library.openBook(id: book.id)
            rejection = ImportAlert(problems)
        case .problems(let problems):
            rejection = ImportAlert(problems)
        }
    }
}

/// A cell of the grid: the "+" cell, or a Book's.
private enum GalleryItem: Hashable {
    case importCell
    case book(String)
}

extension GridHighlight.Arrow {
    fileprivate init?(_ key: KeyEquivalent) {
        switch key {
        case .upArrow: self = .up
        case .downArrow: self = .down
        case .leftArrow: self = .left
        case .rightArrow: self = .right
        default: return nil
        }
    }
}

/// The grid's measurements. Cells are at least `minimumCellWidth` wide, and
/// the columns that fit are counted here because arrow keys need the count.
private enum GalleryLayout {
    static let spacing: CGFloat = 16
    static let minimumCellWidth: CGFloat = 140

    static func columnCount(forWidth width: CGFloat) -> Int {
        let usable = width - 2 * spacing
        return max(1, Int((usable + spacing) / (minimumCellWidth + spacing)))
    }
}

/// The alert for an Import that didn't happen.
private struct ImportAlert {
    let title: String
    let message: String

    /// The alert for the problems of Files Imported; nil if there are none.
    /// Several are listed in one alert, each with its reason.
    init?(_ problems: [ImportProblem]) {
        guard let problem = problems.first else { return nil }
        guard problems.count > 1 else {
            self.init(file: problem.file, error: problem.error)
            return
        }
        self.init(
            title: "Couldn't import \(problems.count) files",
            message: problems.map { "“\($0.file.lastPathComponent)”: \($0.error.reason)" }.joined(separator: "\n"))
    }

    private init(title: String, message: String) {
        self.title = title
        self.message = message
    }

    /// The alert for the one File `url`, which wasn't Imported for `error`.
    private init(file url: URL, error: LibraryError) {
        let name = url.lastPathComponent
        switch error {
        case .Failed(let message):
            title = "Couldn't import “\(name)”"
            self.message = message
        case .UnsupportedFormat:
            title = "Not a PDF or EPUB"
            message = "“\(name)” isn't a PDF or an EPUB."
        case .UnreadablePdf:
            title = "Couldn't read PDF"
            message = "“\(name)” can't be opened. The file may be damaged."
        case .LockedPdf:
            title = "Password-protected"
            message = "Password-protected PDFs aren't supported yet."
        case .InvalidEpub:
            title = "Couldn't read EPUB"
            message = "“\(name)” isn't a valid EPUB."
        case .DrmProtectedEpub:
            title = "DRM-protected"
            message = "This EPUB is DRM-protected."
        case .FixedLayoutEpub:
            title = "Fixed-layout EPUB"
            message = "“\(name)” has a fixed layout, which isn't supported."
        }
    }
}

extension LibraryError {
    /// Why the File wasn't Imported, for a list.
    fileprivate var reason: String {
        switch self {
        case .Failed(let message): message
        case .UnsupportedFormat: "Not a PDF or EPUB."
        case .UnreadablePdf: "Can't be opened. The file may be damaged."
        case .LockedPdf: "Password-protected PDFs aren't supported yet."
        case .InvalidEpub: "Isn't a valid EPUB."
        case .DrmProtectedEpub: "This EPUB is DRM-protected."
        case .FixedLayoutEpub: "Fixed-layout EPUBs aren't supported."
        }
    }
}

/// The toolbar menu choosing how the Gallery is sorted, like the one in Files.
private struct SortMenu: View {
    @Binding var sort: LibrarySort

    var body: some View {
        Menu("Sort", systemImage: "arrow.up.arrow.down") {
            Picker("Sort By", selection: $sort.key) {
                Text("Date Imported").tag(LibrarySort.Key.dateImported)
                Text("Title").tag(LibrarySort.Key.title)
                Text("Recently Read").tag(LibrarySort.Key.recentlyRead)
            }
            Toggle("Reverse Order", isOn: $sort.isReversed)
        }
        .accessibilityIdentifier("sort")
    }
}
