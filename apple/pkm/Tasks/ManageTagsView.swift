import SwiftUI

/// Settings' Tags section: one entry, "Manage Tags", which opens the page of
/// Tags.
struct TagsSettingsSection: View {
    var body: some View {
        Section("Tags") {
            NavigationLink("Manage Tags") { ManageTagsView() }
                .accessibilityIdentifier("manage-tags")
        }
    }
}

/// Every Tag, alphabetically, each `a/b` as one flat row in its Tag Color, which
/// the row's color picker changes and "Default" resets; "No Tags" if there are none.
struct ManageTagsView: View {
    @Environment(TasksModel.self) private var tasks
    @State private var deletion = TagDeletion()

    var body: some View {
        List(tasks.allTags, id: \.name) { tag in
            HStack {
                ColorPicker(selection: color(of: tag), supportsOpacity: false) {
                    Text(tag.name).foregroundStyle(tag.style(default: .primary))
                }
                Button("Default") { tasks.setTagColor(tag.name, to: nil) }
                    .buttonStyle(.borderless)
                    .disabled(tag.color == nil)
            }
            .swipeActions(edge: .trailing) {
                Button("Delete", role: .destructive) { deletion.ask(for: tag.name) }
            }
            .contextMenu {
                Button("Delete", role: .destructive) { deletion.ask(for: tag.name) }
            }
        }
        .confirmationDialog(
            "Delete '\(deletion.pending ?? "")'? It is Removed from every Item.",
            isPresented: asking, titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                if let tag = deletion.pending { tasks.deleteTag(tag) }
                deletion.cancel()
            }
            Button("Cancel", role: .cancel) { deletion.cancel() }
        }
        .overlay { if tasks.allTags.isEmpty { Text("No Tags").foregroundStyle(.secondary) } }
        .navigationTitle("Manage Tags")
    }

    /// The dialog shows while a Tag is pending; dismissing it cancels.
    private var asking: Binding<Bool> {
        Binding(get: { deletion.pending != nil }, set: { if !$0 { deletion.cancel() } })
    }

    /// The picker's color: the Tag Color, or the grey of Default.
    private func color(of tag: TagEntry) -> Binding<Color> {
        Binding(
            get: { tag.color.flatMap(Color.init(tagColor:)) ?? .gray },
            set: {
                // The picker reports often; a pick that changes nothing writes nothing.
                let picked = $0.tagColor
                if picked != tag.color { tasks.setTagColor(tag.name, to: picked) }
            })
    }
}

/// The Tag awaiting the user's answer to "Delete 'work'?"; none when nobody is asked.
struct TagDeletion {
    private(set) var pending: String?

    mutating func ask(for tag: String) { pending = tag }
    mutating func cancel() { pending = nil }
}
