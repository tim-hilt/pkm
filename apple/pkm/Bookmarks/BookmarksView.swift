import SwiftUI

/// The Bookmarks Context: its Bookmarks, newest Saved first, each with Cover, Title and Site, and a
/// its Tags and a mark if it has an Article, narrowed to one Tag from the menu. Tapping one opens its
/// Article in the Reader, or else its page in the system's browser; swipe right or the context menu
/// Edit it (its Details), swipe left or the context menu Delete it, after asking. "+" opens the Save sheet.
struct BookmarksView: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var isSaving = false
    @State private var deleting: Bookmark?

    var body: some View {
        @Bindable var model = model
        List(model.shown, id: \.id) { bookmark in
            Button {
                open(bookmark)
            } label: {
                BookmarkRow(bookmark: bookmark)
            }
            .buttonStyle(.plain)
            .swipeActions(edge: .leading) {
                Button("Edit", systemImage: "pencil") { model.detailsID = bookmark.id }.tint(.blue)
            }
            .swipeActions(edge: .trailing) {
                Button("Delete", systemImage: "trash", role: .destructive) { deleting = bookmark }
            }
            .contextMenu {
                Button("Edit", systemImage: "pencil") { model.detailsID = bookmark.id }
                Button("Delete", systemImage: "trash", role: .destructive) { deleting = bookmark }
            }
        }
        .navigationTitle("Bookmarks")
        .overlay {
            if model.bookmarks.isEmpty {
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
        }
        .addButton("Add Bookmark", identifier: "add-bookmark") { isSaving = true }
        .sheet(isPresented: $isSaving) { SaveSheet(open: open) }
        .sheet(
            item: Binding(
                get: { model.bookmarks.first { $0.id == model.detailsID } },
                set: { model.detailsID = $0?.id })
        ) { BookmarkDetails(id: $0.id) }
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
                    Text(bookmark.site).lineLimit(1)
                }
                .font(.caption).foregroundStyle(.secondary)
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

/// The Save sheet: the Address, then the page's Title to rename before Saving, whether to keep its
/// Article (the app's decision, which the user can flip), and the reason if the page couldn't be
/// read. An Address Saved already opens its Bookmark instead.
private struct SaveSheet: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let open: (Bookmark) -> Void
    @State private var address = ""
    @State private var title = ""
    @State private var keepsArticle = false
    @State private var tags: [String] = []
    @State private var isPickingTags = false
    @State private var draft: SaveDraft?
    @State private var isWorking = false
    /// The share of downloads done, once the page is in.
    @State private var progress: Double?
    @State private var isInvalid = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Address", text: $address)
                        .autocorrectionDisabled()
                        .onSubmit { Task { await prepare() } }
                        .disabled(draft != nil)
                        .accessibilityIdentifier("bookmark-address")
                        #if os(iOS)
                            .textInputAutocapitalization(.never)
                            .keyboardType(.URL)
                        #endif
                } footer: {
                    if isInvalid { Text("This is not a web address.") }
                }
                if let draft {
                    Section {
                        TextField("Title", text: $title).accessibilityIdentifier("bookmark-title")
                        TagsRow(tags: tags) { isPickingTags = true }
                        if !draft.article.markdown.isEmpty {
                            Toggle("Keep Article", isOn: $keepsArticle).accessibilityIdentifier("keep-article")
                        }
                    } footer: {
                        if let failure = draft.failure { Text("Saved with its address only: \(failure)") }
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Save Bookmark")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if let draft {
                        Button("Save") {
                            model.save(draft, title: title, tags: tags, keepsArticle: keepsArticle)
                            dismiss()
                        }
                    } else {
                        Button("Next") { Task { await prepare() } }
                            .disabled(isWorking || address.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
            .overlay {
                if isWorking {
                    if let progress { ProgressView(value: progress).padding(40) } else { ProgressView() }
                }
            }
        }
        .sheet(isPresented: $isPickingTags) {
            TagPicker(selected: tags) {
                tags = $0
                isPickingTags = false
            }
        }
        #if os(macOS)
            .frame(minWidth: 380, minHeight: 220)
        #endif
    }

    private func prepare() async {
        guard !isWorking else { return }
        isWorking = true
        isInvalid = false
        progress = nil
        defer { isWorking = false }
        switch await model.prepare(address, progress: { progress = $0 }) {
        case .invalid: isInvalid = true
        case .existing(let bookmark):
            dismiss()
            open(bookmark)
        case .draft(let ready):
            title = ready.details.title
            keepsArticle = ready.article.readsLikeArticle
            draft = ready
        }
    }
}
