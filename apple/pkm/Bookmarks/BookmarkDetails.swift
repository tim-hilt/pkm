import SwiftUI

extension View {
    /// Bookmark Details of `BookmarksModel.detailsID` as a sheet over this view while it `isInFront`:
    /// on the Mac a sheet of a view under a pushed one waits until it shows again.
    func bookmarkDetailsSheet(isInFront: Bool) -> some View {
        modifier(BookmarkDetailsSheet(isInFront: isInFront))
    }
}

private struct BookmarkDetailsSheet: ViewModifier {
    @Environment(BookmarksModel.self) private var model
    let isInFront: Bool

    func body(content: Content) -> some View {
        content.sheet(
            item: Binding(
                get: { isInFront ? model.bookmarks.first { $0.id == model.detailsID } : nil },
                set: { model.detailsID = $0?.id })
        ) { BookmarkDetails(id: $0.id) }
    }
}

/// Bookmark Details: the Cover, the Title and Tags to change, what the page said about itself
/// (Site, Address, Author, Published, Excerpt, shown as found), when it was Saved, and the actions on
/// the Bookmark: Read, Open Page, Add or Drop Article, Delete. Dropping and Deleting always ask.
/// A sheet over the list; the Title is saved when it is left.
struct BookmarkDetails: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(TasksModel.self) private var tasks
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    let id: String
    @State private var title = ""
    @State private var isPickingTags = false
    @State private var isConfirmingDrop = false
    @State private var isConfirmingDelete = false
    @State private var isAddingArticle = false
    @State private var addingFailed = false

    private var bookmark: Bookmark? { model.bookmarks.first { $0.id == id } }

    var body: some View {
        NavigationStack {
            if let bookmark {
                Form {
                    header(bookmark)
                    found(bookmark)
                    actions(bookmark)
                }
                .formStyle(.grouped)
                .navigationTitle("Bookmark")
                #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
            }
        }
        .onAppear { title = bookmark?.title ?? "" }
        .onDisappear { model.rename(id: id, to: title) }
        .onChange(of: bookmark == nil) { _, gone in if gone { dismiss() } }
        .sheet(isPresented: $isPickingTags) {
            TagPicker(selected: bookmark?.tags ?? []) {
                model.setTags($0, of: id)
                tasks.refreshTags()
                isPickingTags = false
            }
        }
        #if os(macOS)
            .frame(minWidth: 420, minHeight: 480)
        #endif
        .accessibilityIdentifier("bookmark-details-sheet")
    }

    private func header(_ bookmark: Bookmark) -> some View {
        Section {
            if let cover = model.cover(of: bookmark) {
                Image(decorative: cover, scale: 1).resizable().scaledToFill()
                    .frame(maxWidth: .infinity, maxHeight: 160).clipShape(RoundedRectangle(cornerRadius: 8))
                    .listRowInsets(EdgeInsets())
            }
            TextField("Title", text: $title, axis: .vertical)
                .onSubmit { model.rename(id: id, to: title) }
                .accessibilityIdentifier("bookmark-details-title")
            TagsRow(tags: bookmark.tags) { isPickingTags = true }
        }
    }

    /// What was found on the page and when it was Saved; an empty one is left out.
    private func found(_ bookmark: Bookmark) -> some View {
        Section {
            row("Site", bookmark.site)
            row("Address", bookmark.address)
            row("Author", bookmark.author)
            row("Published", bookmark.published)
            row("Saved", Self.saved(bookmark.saved))
            row("Excerpt", bookmark.excerpt)
        }
    }

    @ViewBuilder
    private func row(_ label: LocalizedStringKey, _ value: String) -> some View {
        if !value.isEmpty {
            LabeledContent(label) { Text(value).multilineTextAlignment(.trailing).textSelection(.enabled) }
        }
    }

    /// The Saved time (UTC ISO 8601) as the user's date and time; the text itself if it isn't one.
    private static func saved(_ text: String) -> String {
        (try? Date(text, strategy: .iso8601)).map { $0.formatted(date: .abbreviated, time: .shortened) } ?? text
    }

    private func actions(_ bookmark: Bookmark) -> some View {
        Section {
            if bookmark.hasArticle {
                Button("Read", systemImage: "book") {
                    dismiss()
                    model.openID = id
                }
            }
            if let url = URL(string: bookmark.address) {
                Button("Open Page", systemImage: "safari") { openURL(url) }
            }
            if bookmark.hasArticle {
                Button("Drop Article", systemImage: "minus.circle") { isConfirmingDrop = true }
                    .confirmationDialog(
                        "Drop the Article? Its Markup and Reading Progress go with it.", isPresented: $isConfirmingDrop,
                        titleVisibility: .visible
                    ) {
                        Button("Drop Article", role: .destructive) { model.dropArticle(id: id) }
                    }
            } else {
                Button("Add Article", systemImage: "plus.circle") {
                    Task {
                        isAddingArticle = true
                        addingFailed = !(await model.addArticle(to: bookmark))
                        isAddingArticle = false
                    }
                }
                .disabled(isAddingArticle)
            }
            Button("Delete", systemImage: "trash", role: .destructive) { isConfirmingDelete = true }
                .confirmationDialog(
                    "Delete “\(bookmark.title)”?", isPresented: $isConfirmingDelete, titleVisibility: .visible
                ) {
                    Button("Delete", role: .destructive) { model.delete(id: id) }
                }
        } footer: {
            if addingFailed { Text("The page has no text to keep, or couldn't be downloaded.") }
        }
    }
}
