import SwiftUI

/// The Bookmarks Context: its Bookmarks, newest Saved first, each with Cover, Title and Site. Tapping
/// one opens its page in the system's browser; swipe left or the context menu Delete it, after
/// asking. "+" opens the Save sheet.
struct BookmarksView: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var isSaving = false
    @State private var deleting: Bookmark?

    var body: some View {
        List(model.bookmarks, id: \.id) { bookmark in
            Button {
                open(bookmark)
            } label: {
                BookmarkRow(bookmark: bookmark)
            }
            .buttonStyle(.plain)
            .swipeActions(edge: .trailing) {
                Button("Delete", systemImage: "trash", role: .destructive) { deleting = bookmark }
            }
            .contextMenu {
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
        .addButton("Add Bookmark", identifier: "add-bookmark") { isSaving = true }
        .sheet(isPresented: $isSaving) { SaveSheet(open: open) }
        .confirmationDialog(
            deleting.map { "Delete “\($0.title)”?" } ?? "",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { bookmark in
            Button("Delete", role: .destructive) { model.delete(id: bookmark.id) }
        }
        .accessibilityIdentifier("bookmarks-list")
    }

    /// Open Page: the Bookmark's Address in the system's browser.
    private func open(_ bookmark: Bookmark) {
        if let url = URL(string: bookmark.address) { openURL(url) }
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
                Text(bookmark.site).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// The Save sheet: the Address, then the page's Title to rename before Saving, and the reason if
/// the page couldn't be read. An Address Saved already opens its Bookmark instead.
private struct SaveSheet: View {
    @Environment(BookmarksModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let open: (Bookmark) -> Void
    @State private var address = ""
    @State private var title = ""
    @State private var draft: SaveDraft?
    @State private var isWorking = false
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
                            model.save(draft, title: title)
                            dismiss()
                        }
                    } else {
                        Button("Next") { Task { await prepare() } }
                            .disabled(isWorking || address.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
            .overlay { if isWorking { ProgressView() } }
        }
        #if os(macOS)
            .frame(minWidth: 380, minHeight: 220)
        #endif
    }

    private func prepare() async {
        guard !isWorking else { return }
        isWorking = true
        isInvalid = false
        defer { isWorking = false }
        switch await model.prepare(address) {
        case .invalid: isInvalid = true
        case .existing(let bookmark):
            dismiss()
            open(bookmark)
        case .draft(let ready):
            title = ready.details.title
            draft = ready
        }
    }
}
