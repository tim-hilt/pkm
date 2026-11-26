import SwiftUI

/// What the Share extension shows: the Save sheet for the page that was shared, into the Bookmarks
/// the app takes in when it comes to the front, without the app opening.
struct ShareRoot: View {
    let items: [NSExtensionItem]
    let complete: () -> Void
    @State private var model: BookmarksModel?
    @State private var page: SharedPage?
    @State private var tags: [String] = []
    @State private var failed = false

    var body: some View {
        Group {
            if let model, let page {
                SaveSheet(page: page, found: nil, close: complete) { ShareTagsField(tags: $0, known: tags) }
                    .environment(model)
            } else if failed {
                ContentUnavailableView(
                    "Can't Save This", systemImage: "bookmark.slash",
                    description: Text("Only web pages can be saved.")
                )
                .overlay(alignment: .topTrailing) { Button("Cancel", action: complete).padding() }
            } else {
                ProgressView()
            }
        }
        .task {
            guard let handoff = ShareHandoff.live, let inbox = try? handoff.openInbox(),
                let shared = await SharedPage(items: items)
            else {
                failed = true
                return
            }
            let published = handoff.published()
            // Two downloads at a time: an extension may use little memory.
            let model = BookmarksModel(store: inbox, parallelDownloads: 2)
            model.savedElsewhere = published.bookmarks.map(\.bookmark)
            tags = published.tags
            self.model = model
            page = shared
        }
    }
}

/// The Tags line of the Save sheet in the extension: the Tags the app published, to pick from or
/// create.
struct ShareTagsField: View {
    @Binding var tags: [String]
    let known: [String]
    @State private var isPicking = false
    @State private var query = ""

    var body: some View {
        Button {
            isPicking = true
        } label: {
            HStack {
                Image(systemName: "tag").foregroundStyle(.secondary)
                if tags.isEmpty {
                    Text("Add Tags").foregroundStyle(.secondary)
                } else {
                    Text(tags.joined(separator: ", "))
                }
                Spacer(minLength: 0)
            }
            .lineLimit(1)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .sheet(isPresented: $isPicking) { picker }
    }

    /// The published Tags (by Relevance), then those created here.
    private var names: [String] { (known + tags).uniqued().filter { query.isEmpty || $0.contains(query) } }

    private var canCreate: Bool { isValidTag(tag: query) && !names.contains(query) }

    private var picker: some View {
        NavigationStack {
            List {
                if canCreate {
                    Button("Create Tag '\(query)'") {
                        tags.append(query)
                        query = ""
                    }
                }
                ForEach(names, id: \.self) { tag in
                    Button {
                        if let index = tags.firstIndex(of: tag) { tags.remove(at: index) } else { tags.append(tag) }
                    } label: {
                        HStack {
                            Text(tag)
                            Spacer()
                            Image(systemName: tags.contains(tag) ? "checkmark.circle.fill" : "circle")
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(tags.contains(tag) ? .isSelected : [])
                }
            }
            .navigationTitle("Tags")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .searchable(text: $query, prompt: "Search or create")
            .autocorrectionDisabled()
            #if os(iOS)
                .textInputAutocapitalization(.never)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { isPicking = false } }
            }
        }
        #if os(macOS)
            .frame(minWidth: 320, minHeight: 360)
        #endif
        .presentationDetents([.large])
    }
}

extension Sequence where Element: Hashable {
    fileprivate func uniqued() -> [Element] {
        var seen = Set<Element>()
        return filter { seen.insert($0).inserted }
    }
}
