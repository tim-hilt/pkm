import SwiftUI

/// The Save sheet, in the app and in the Share extension: the Address, then the page's Title to
/// rename before Saving, its Tags (`tagsField` edits them), whether to keep its Article (the app's
/// decision, which the user can flip), the progress of the downloads, and the reason if the page
/// couldn't be read. `page` is what a Share sheet handed over, prepared at once; nil in the app,
/// where the Address is typed. For an Address Saved already `found` is told of that Bookmark and
/// the sheet is `close`d; without `found` the sheet shows it and offers Add Article if it has none.
struct SaveSheet<TagsField: View>: View {
    @Environment(BookmarksModel.self) private var model
    let page: SharedPage?
    let found: ((Bookmark) -> Void)?
    let close: () -> Void
    @ViewBuilder let tagsField: (Binding<[String]>) -> TagsField
    @State private var address = ""
    @State private var title = ""
    @State private var keepsArticle = false
    @State private var tags: [String] = []
    @State private var draft: SaveDraft?
    @State private var saved: Bookmark?
    @State private var isWorking = false
    /// The share of downloads done, once the page is in.
    @State private var progress: Double?
    @State private var isInvalid = false
    @State private var addingFailed = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Address", text: $address)
                        .autocorrectionDisabled()
                        .onSubmit { Task { await prepare() } }
                        .disabled(page != nil || draft != nil || saved != nil)
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
                        tagsField($tags)
                        if !draft.article.markdown.isEmpty {
                            Toggle("Keep Article", isOn: $keepsArticle).accessibilityIdentifier("keep-article")
                        }
                    } footer: {
                        if let failure = draft.failure { Text("Saved with its address only: \(failure)") }
                    }
                }
                if let saved {
                    Section {
                        Text(saved.title)
                        if saved.hasArticle {
                            Label("Saved already, with its Article.", systemImage: "checkmark")
                        } else {
                            Label("Saved already.", systemImage: "checkmark")
                            Button("Add Article") { Task { await addArticle(to: saved) } }
                                .disabled(isWorking)
                        }
                    } footer: {
                        if addingFailed { Text("The page has no text to keep.") }
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Save Bookmark")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel", action: close) }
                ToolbarItem(placement: .confirmationAction) {
                    if let draft {
                        Button("Save") {
                            model.save(draft, title: title, tags: tags, keepsArticle: keepsArticle)
                            close()
                        }
                    } else if saved != nil {
                        Button("Done", action: close)
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
        .task {
            guard let page else { return }
            address = page.address
            await prepare()
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
        switch await model.prepare(address, html: page?.html, progress: { progress = $0 }) {
        case .invalid: isInvalid = true
        case .existing(let bookmark):
            if let found {
                found(bookmark)
                close()
            } else {
                saved = bookmark
            }
        case .draft(let ready):
            title = ready.details.title
            keepsArticle = ready.article.readsLikeArticle
            draft = ready
        }
    }

    private func addArticle(to bookmark: Bookmark) async {
        isWorking = true
        addingFailed = false
        progress = nil
        defer { isWorking = false }
        if await model.addArticle(to: bookmark, html: page?.html) {
            saved = Bookmark(copying: bookmark, hasArticle: true)
        } else {
            addingFailed = true
        }
    }
}

extension Bookmark {
    /// The Bookmark as it is with `hasArticle` changed.
    fileprivate init(copying other: Bookmark, hasArticle: Bool) {
        self.init(
            id: other.id, address: other.address, title: other.title, site: other.site, cover: other.cover,
            excerpt: other.excerpt, author: other.author, published: other.published, saved: other.saved,
            hasArticle: hasArticle, tags: other.tags, progress: other.progress)
    }
}
