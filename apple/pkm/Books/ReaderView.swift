import SwiftUI

/// Opens a Book's File and shows its pages in a single continuously
/// scrolling vertical column, titled with the Book's Title. Whatever the
/// Book's Format, the Reader only knows its book view through
/// `ReaderBookView` and its positions as a `Place`. Must sit in a
/// `NavigationStack`, whose bar holds the "Books" button (⌘[ on macOS) that Goes to Gallery.
///
/// Immersive Mode hides everything but the pages. On iOS a
/// single tap toggles it and dragging the pages enters it; on macOS it is the
/// native full screen, with the toolbar shown on hover.
/// Scrolling never exits it, nor minimizes the bars, and the Reader always
/// opens with its UI showing. Toggling it leaves the pages where they are.
///
/// Pages are drawn per the Page Style in Settings, and redrawn live when it changes.
/// ⇧⌘D switches to the opposite of what is showing; on macOS the View menu also
/// has the three Page Styles.
///
/// Selecting text and choosing "Highlight" in the selection menu (⌃⌘H on macOS)
/// adds a Highlight in the default color, stored in the Library, never in the
/// File. Highlights are drawn into the pages, so Dark Pages reverse them.
/// Clicking or tapping a Highlight, or its Comment icon, opens a popover (on iPhone too) to recolor it, write its Comment or remove it, all stored at once.
///
/// A Comment can also sit on an empty spot of a page: "Add Comment" in the context menu
/// (macOS) or after a long press (iOS). Its popover opens in editing and also has
/// "Remove Comment"; emptying the Comment removes it. Page Comments can't be moved.
///
/// Shows `loaded`, which is read before the Reader opens (see `LoadedBook`).
/// Opens at the Book's Reading Progress at fit-to-width, or at page 1. It saves Reading Progress (see `ReadingProgressSaver`) when opened, after scrolling stops,
/// when it disappears and when the scene goes to the background. Reading
/// Progress from another Device never moves an open Reader.
///
/// A trailing toolbar button opens the Table of Contents, in a popover (a sheet
/// on iPhone), unless the Book has none. It shows where the top of the
/// page is, and choosing an entry is a Jump. After a Jump, a floating capsule
/// "Back to p. 42" returns to where the reader was; it also shows in Immersive
/// Mode, where it is the only way back. It goes once the reader has moved on:
/// about a page from where the Jump landed, or back at where it started. Following a link in the Book is a
/// Jump too, and tapping a link doesn't switch Immersive Mode.
///
/// A trailing toolbar cog opens a popover, on iPhone too, to change Page Style
/// while reading, unless Immersive Mode hides it.
///
/// A trailing toolbar button opens the native find bar for Book Search,
/// unless Immersive Mode hides it. ⌘F opens it too: on iOS with a keyboard, on
/// macOS from Edit > Find, which also has ⌘G and ⇧⌘G. All matches are listed
/// with their surrounding text and page numbers (none in EPUBs): in a leading sidebar while the
/// find bar is open and has matches, or on compact widths in a sheet that the
/// find bar's options open.
struct ReaderView: View {
    let book: Book
    let loaded: LoadedBook
    /// Go to Gallery: leaves the Reader.
    let onGoToGallery: () -> Void
    @Environment(LibraryModel.self) private var library
    @Environment(ReaderSettings.self) private var settings
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var handle = ReaderHandle()
    @State private var saver: ReadingProgressSaver?
    @State private var isShowingContents = false
    @State private var isShowingPageStyle = false
    @State private var results = SearchResults()
    @State private var highlights: [Highlight]
    @State private var isShowingResults = false
    @State private var pageComments: [PageComment]
    @State private var openMarkup: OpenMarkup?
    /// The Back place; nil when there is none.
    @State private var backTarget: BackTarget?
    /// Immersive Mode; on macOS, whether the window is in full screen.
    @State private var isImmersive = false

    init(book: Book, loaded: LoadedBook, onGoToGallery: @escaping () -> Void) {
        self.book = book
        self.loaded = loaded
        self.onGoToGallery = onGoToGallery
        _highlights = State(initialValue: loaded.highlights)
        _pageComments = State(initialValue: loaded.pageComments)
    }

    var body: some View {
        HStack(spacing: 0) {
            if showsResultsSidebar {
                SearchResultsList(results: results)
                    .frame(width: 320)
                    .transition(.move(edge: .leading))
                Divider()
            }
            pages
        }
        .animation(.default, value: showsResultsSidebar)
        .sheet(isPresented: $isShowingResults) { SearchResultsSheet(results: results) }
        .task {
            if saver == nil { saver = ReadingProgressSaver(library: library, book: book, handle: handle) }
        }
        .onDisappear { saver?.saveIfMoved() }
        .onChange(of: scenePhase) { if scenePhase == .background { saver?.saveIfMoved() } }
        // Buttons only: no title (it's in the settings info).
        // Immersive Mode hides the bar, buttons and all: removing them instead, iOS would morph
        // their glass across the bar as it fades.
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button("Books", systemImage: "chevron.backward", action: onGoToGallery)
                    .accessibilityIdentifier("back")
            }
            ToolbarItem(placement: .primaryAction) {
                Button("Find", systemImage: "magnifyingglass") { handle.view?.presentFind() }
                    #if os(iOS)
                        .keyboardShortcut("f", modifiers: .command)
                    #endif
            }
            if let contents = availableContents {
                ToolbarItem(placement: .primaryAction) {
                    Button("Table of Contents", systemImage: "list.bullet") { isShowingContents = true }
                        .popover(isPresented: $isShowingContents) {
                            TableOfContentsView(contents: contents, position: position(in: contents)) {
                                handle.view?.jump(to: $0)
                            }
                            .presentationCompactAdaptation(.sheet)
                        }
                }
            }
            ToolbarItem(placement: .primaryAction) {
                Button("Page Style", systemImage: "gearshape") { isShowingPageStyle = true }
                    .popover(isPresented: $isShowingPageStyle) { PageStylePopover(book: book) }
            }
        }
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            // The bar's background stays: it is the scroll edge effect, which keeps the
            // glass buttons light over light pages rather than tinted by the text under them.
            .toolbarVisibility(isImmersive ? .hidden : .automatic, for: .navigationBar)
            .navigationBarNeverMinimizes()
            .statusBarHidden(isImmersive)
            // Hiding system overlays also lets the home indicator fade.
            .persistentSystemOverlays(isImmersive ? .hidden : .automatic)
            .animation(.default, value: isImmersive)
            .background(switchPageStyleShortcut)
        #else
            .onAppear { isImmersive = NSApp.keyWindow?.styleMask.contains(.fullScreen) ?? false }
            .onReceive(NotificationCenter.default.publisher(for: NSWindow.didEnterFullScreenNotification)) { _ in
                isImmersive = true
            }
            .onReceive(NotificationCenter.default.publisher(for: NSWindow.didExitFullScreenNotification)) { _ in
                isImmersive = false
            }
            .background(GoToGalleryShortcut(action: onGoToGallery))
            .focusedSceneValue(\.reader, handle)
            .focusedSceneValue(\.backTarget, backTarget)
            .focusedSceneValue(\.readerColorScheme, colorScheme)
            .focusedSceneValue(\.showTableOfContents, availableContents == nil ? nil : $isShowingContents)
            .windowToolbarFullScreenVisibility(.onHover)
            .toolbar(removing: .title)
            .toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
        #endif
    }

    #if os(iOS)
        /// Invisible button giving ⇧⌘D to iPad keyboards; on macOS the View menu has it.
        private var switchPageStyleShortcut: some View {
            Button("Switch Light and Dark Pages") { settings.switchToOppositePageStyle(systemScheme: colorScheme) }
                .keyboardShortcut("d", modifiers: [.shift, .command])
                .opacity(0)
                .accessibilityHidden(true)
        }
    #endif

    private var inputs: ReaderInputs {
        ReaderInputs(
            appearance: settings.pageAppearance(systemScheme: colorScheme), highlights: highlights,
            pageComments: pageComments, pageCommentColor: library.highlightColors[0].tint,
            handle: handle, results: results,
            place: book.place,
            contents: loaded.contents, textSize: settings.epubTextSize,
            callbacks: ReaderCallbacks(
                onSingleTap: toggleImmersive, onOpen: { saver?.opened() }, onDragScroll: enterImmersive,
                onScroll: { saver?.scrolled() },
                onHighlight: highlight,
                onHighlightTap: { openMarkup = OpenMarkup(target: .highlight($0.id), point: $1) },
                onAddComment: addPageComment,
                onPageCommentTap: { openMarkup = OpenMarkup(target: .pageComment($0.id), point: $1) },
                onTextSizeChange: { settings.epubTextSize = $0 }, onBackChange: { backTarget = $0 }),
            onShowResults: { isShowingResults = true })
    }

    private var pages: some View {
        ReaderContent(book: loaded.file, inputs: inputs)
            // Inside `ignoresSafeArea`, so the anchor shares the reader's frame,
            // whose coordinates the tap points are in.
            .popover(
                item: $openMarkup,
                attachmentAnchor: .rect(
                    .rect(CGRect(origin: openMarkup?.point ?? .zero, size: CGSize(width: 1, height: 1))))
            ) { open in
                popover(for: open)
            }
            #if os(iOS)
                // Under the bars and the home indicator, so Immersive Mode changes
                // only the safe area, not the frame, and the pages stay put. Without
                // the bottom, the window's background would strip over the bottom
                // safe area, whatever the Appearance.
                .ignoresSafeArea(.container, edges: [.top, .bottom])
            #endif
            .overlay(alignment: .bottom) {
                if let backTarget {
                    BackCapsule(target: backTarget) { handle.view?.goBack() }
                        .padding(.bottom)
                        .transition(.opacity)
                }
            }
            .animation(.default, value: backTarget)
    }

    /// The popover of the Highlight or page Comment `open`; nothing if it is gone.
    @ViewBuilder private func popover(for open: OpenMarkup) -> some View {
        switch open.target {
        case .highlight(let id):
            if let highlight = highlights.first(where: { $0.id == id }) {
                HighlightPopover(
                    colors: library.highlightColors, color: highlight.tint, comment: highlight.comment,
                    onColor: { recolor(highlight, to: $0) }, onComment: { setComment($0, of: highlight) },
                    onRemove: { remove(highlight) }
                )
                .presentationCompactAdaptation(.popover)
            }
        case .pageComment(let id):
            if let comment = pageComments.first(where: { $0.id == id }) {
                PageCommentPopover(
                    text: comment.text, startsEditing: open.startsEditing, onText: { setText($0, of: comment) },
                    onRemove: { remove(comment) }
                )
                .presentationCompactAdaptation(.popover)
            }
        }
    }

    /// Whether the matches are listed in a sidebar: while the find bar is open
    /// and has matches, unless the width is compact, which has a sheet instead.
    private var showsResultsSidebar: Bool { results.isListable && sizeClass != .compact }

    /// The Table of Contents, unless it is empty.
    private var availableContents: TableOfContents? {
        loaded.contents.isEmpty() ? nil : loaded.contents
    }

    /// Where the top of the viewport is, as of now: the Table of Contents is
    /// built when it opens, so this is read then.
    private func position(in contents: TableOfContents) -> Position? {
        handle.view?.topOfViewport.flatMap(contents.position(place:))
    }

    /// Adds a Highlight over `anchor`; it shows once the Library has stored it.
    private func highlight(_ anchor: Anchor, quotedText: String) {
        Task {
            guard let added = await library.addHighlight(anchor, quotedText: quotedText, to: book)
            else { return }
            highlights.append(added)
        }
    }

    /// Recolors `highlight`; it shows at once, and the Library stores it.
    private func recolor(_ highlight: Highlight, to color: RGB) {
        guard let index = highlights.firstIndex(where: { $0.id == highlight.id }) else { return }
        highlights[index].color = Rgb(color)
        Task { await library.setHighlightColor(color, of: highlight, in: book) }
    }

    /// Sets the Comment of `highlight`, empty to remove it; it shows at once, and the Library stores it.
    private func setComment(_ comment: String, of highlight: Highlight) {
        guard let index = highlights.firstIndex(where: { $0.id == highlight.id }),
            (highlights[index].comment ?? "") != comment
        else { return }
        highlights[index].comment = comment.isEmpty ? nil : comment
        Task { await library.setComment(comment, of: highlight, in: book) }
    }

    /// Removes `highlight` and closes its popover; the Library stores the removal.
    private func remove(_ highlight: Highlight) {
        openMarkup = nil
        highlights.removeAll { $0.id == highlight.id }
        Task { await library.removeHighlight(highlight, from: book) }
    }

    /// Adds a page Comment at `place`, empty, and opens its popover in editing; it
    /// shows once the Library has stored it.
    private func addPageComment(at place: Place) {
        Task {
            guard let added = await library.addPageComment(at: place, to: book),
                let point = await handle.view?.popoverPoint(of: added)
            else { return }
            pageComments.append(added)
            openMarkup = OpenMarkup(target: .pageComment(added.id), point: point, startsEditing: true)
        }
    }

    /// Sets the text of `comment`; empty removes it and closes its popover. It shows
    /// at once, and the Library stores it.
    private func setText(_ text: String, of comment: PageComment) {
        guard let index = pageComments.firstIndex(where: { $0.id == comment.id }) else { return }
        if text.isEmpty {
            openMarkup = nil
            pageComments.remove(at: index)
        } else if pageComments[index].text != text {
            pageComments[index].text = text
        } else {
            return
        }
        Task { await library.setText(text, of: comment, in: book) }
    }

    /// Removes the page Comment `comment` and closes its popover; the Library stores the removal.
    private func remove(_ comment: PageComment) {
        openMarkup = nil
        pageComments.removeAll { $0.id == comment.id }
        Task { await library.removePageComment(comment, from: book) }
    }

    private func enterImmersive() {
        #if os(iOS)
            isImmersive = true
        #endif
    }

    private func toggleImmersive() {
        #if os(iOS)
            isImmersive.toggle()
        #endif
    }
}

#if os(iOS)
    extension View {
        /// Keeps iOS 27+ from minimizing the navigation bar while the content scrolls.
        @ViewBuilder fileprivate func navigationBarNeverMinimizes() -> some View {
            if #available(iOS 27, *) {
                toolbarMinimizationBehavior(.never, for: .navigationBar)
            } else {
                self
            }
        }
    }
#endif

/// The Highlight or page Comment whose popover is open, and the spot it opened
/// from, in the pages' coordinates with the origin at the top-left. A new page
/// Comment opens in editing.
private struct OpenMarkup: Identifiable {
    enum Target: Hashable {
        case highlight(String)
        case pageComment(String)
    }

    let target: Target
    let point: CGPoint
    var startsEditing = false

    var id: Target { target }
}

/// The floating "Back to p. 42" button, on a native material.
private struct BackCapsule: View {
    let target: BackTarget
    let action: () -> Void

    var body: some View {
        Button("Back to \(target.name)", systemImage: "arrow.uturn.backward", action: action)
            .font(.subheadline.weight(.medium))
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: .capsule)
            .buttonStyle(.plain)
            .accessibilityLabel("Back to \(target.spokenName)")
    }
}

#if os(macOS)
    /// Invisible button that makes ⌘[ Go to Gallery, as in other macOS apps.
    private struct GoToGalleryShortcut: View {
        let action: () -> Void

        var body: some View {
            Button("Go to Gallery", action: action)
                .keyboardShortcut("[", modifiers: .command)
                .opacity(0)
                .accessibilityHidden(true)
        }
    }
#endif
