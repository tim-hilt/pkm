import SwiftUI

/// The Bookmarks Context: its Bookmarks, newest Saved first, each with Cover, Title, Site, Tags and a
/// mark if it has an Article, narrowed to one Tag from the menu. Tapping one opens its Article in the
/// Reader, or else its page in the system's browser; swipe right or the context menu Edit it (its
/// Details), swipe left or the context menu Delete it, after asking. "+" opens the Save sheet.
/// Searching lists the matches instead, each with a snippet of where the words were found.
struct BookmarksView: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var isSaving = false
    /// The Bookmark the Save sheet found Saved already, whose Details show once the sheet is gone.
    @State private var existing: String?
    @State private var deleting: Bookmark?

    var body: some View {
        @Bindable var model = model
        List(rows, id: \.bookmark.id) { row in
            let bookmark = row.bookmark
            Button {
                open(bookmark)
            } label: {
                BookmarkRow(bookmark: bookmark, found: row.found)
            }
            .buttonStyle(.plain)
            .swipeActions(edge: .leading) {
                Button("Edit", systemImage: "pencil") { model.detailsID = bookmark.id }.tint(.accentColor)
            }
            .swipeActions(edge: .trailing) {
                Button("Delete", systemImage: "trash", role: .destructive) { deleting = bookmark }
            }
            .contextMenu {
                Button("Edit", systemImage: "pencil") { model.detailsID = bookmark.id }
                Button("Delete", systemImage: "trash", role: .destructive) { deleting = bookmark }
            }
        }
        .listStyle(.plain)
        .searchable(text: $model.searchText)
        #if os(iOS)
            .searchToolbarBehavior(.minimize)
        #endif
        .overlay {
            if isSearching {
                if model.results.isEmpty { ContentUnavailableView.search(text: model.searchText) }
            } else if model.bookmarks.isEmpty {
                ContentUnavailableView(
                    "No Bookmarks", systemImage: "bookmark",
                    description: Text("Tap + and enter the address of a page to Save it."))
            }
        }
        .toolbar {
            if !model.tagsInUse.isEmpty {
                ToolbarItem {
                    Menu("Tag", systemImage: "line.3.horizontal.decrease") {
                        Picker("Tag", selection: $model.tagFilter) {
                            Text("All Bookmarks").tag(String?.none)
                            ForEach(model.tagsInUse, id: \.self) { Text($0).tag(Optional($0)) }
                        }
                    }
                    .accessibilityIdentifier("bookmark-tag-filter")
                }
            }
            #if os(iOS)
                // At the top, so that the bottom stays free for the "+", as in Notes and Books; after the own controls.
                DefaultToolbarItem(kind: .search, placement: .topBarTrailing)
                SettingsToolbarItem()
            #endif
        }
        .addButton(
            "Add Bookmark", identifier: "add-bookmark", isCovered: isSaving || model.detailsID != nil
        ) { isSaving = true }
        .sheet(isPresented: $isSaving, onDismiss: showExisting) { AppSaveSheet { existing = $0.id } }
        .bookmarkDetailsSheet(isInFront: model.openID == nil)
        .navigationDestination(for: String.self) { ArticleReader(id: $0) }
        .confirmationDialog(
            deleting.map { "Delete “\($0.title)”?" } ?? "",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { bookmark in
            Button("Delete", role: .destructive) { model.delete(id: bookmark.id) }
        }
        .accessibilityIdentifier("bookmarks-list")
    }

    private var isSearching: Bool { !model.searchText.isEmpty }

    /// The list's rows: the Bookmarks that match, or else those the Tag filter leaves.
    private var rows: [Row] {
        isSearching
            ? model.results.map { Row(bookmark: $0.bookmark, found: $0) }
            : model.shown.map { Row(bookmark: $0, found: nil) }
    }

    private struct Row {
        let bookmark: Bookmark
        /// What Search found; nil when not searching.
        let found: BookmarkFound?
    }

    private func showExisting() {
        model.detailsID = existing
        existing = nil
    }

    /// Opens the Bookmark: its Article in the Reader, or else its page in the system's browser.
    private func open(_ bookmark: Bookmark) {
        if bookmark.hasArticle {
            model.openID = bookmark.id
        } else if let url = URL(string: bookmark.address) {
            openURL(url)
        }
    }
}

private struct BookmarkRow: View {
    @Environment(BookmarksModel.self) private var model
    let bookmark: Bookmark
    /// What Search found in it, to show a snippet of; nil when not searching.
    let found: BookmarkFound?

    var body: some View {
        HStack(spacing: 12) {
            Group {
                if let cover = model.cover(of: bookmark) {
                    Image(decorative: cover, scale: 1).resizable().scaledToFill()
                } else {
                    Image(systemName: "bookmark").foregroundStyle(.secondary)
                }
            }
            .frame(width: 56, height: 56)
            .background(.quaternary)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 2) {
                Text(bookmark.title).lineLimit(2)
                HStack(spacing: 4) {
                    if bookmark.hasArticle {
                        Image(systemName: "doc.text").accessibilityLabel("Article")
                    }
                    if let progress = bookmark.progress {
                        Text("\(Int(progress.percent)) %").monospacedDigit()
                    }
                    Text(bookmark.site).lineLimit(1)
                }
                .font(.caption).foregroundStyle(.secondary)
                if let found, !found.snippet.isEmpty {
                    Text(
                        AttributedString(
                            snippet: found.snippet, matchedStart: found.matchedStart, matchedEnd: found.matchedEnd)
                    )
                    .lineLimit(2).font(.subheadline).foregroundStyle(.secondary)
                }
                if !bookmark.tags.isEmpty {
                    HStack(spacing: 4) { ForEach(bookmark.tags, id: \.self) { TagPill(tag: $0) } }
                        .font(.caption2).lineLimit(1)
                }
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// The Save sheet with the app's Tag picker; for an Address Saved already it closes and shows that
/// Bookmark's Details.
private struct AppSaveSheet: View {
    @Environment(\.dismiss) private var dismiss
    let found: (Bookmark) -> Void

    var body: some View {
        SaveSheet(page: nil, found: found, close: { dismiss() }, tagsField: { AppTagsField(tags: $0) })
    }
}

private struct AppTagsField: View {
    @Binding var tags: [String]
    @State private var isPicking = false

    var body: some View {
        TagsRow(tags: tags) { isPicking = true }
            .sheet(isPresented: $isPicking) {
                TagPicker(selected: tags) {
                    tags = $0
                    isPicking = false
                }
            }
    }
}
