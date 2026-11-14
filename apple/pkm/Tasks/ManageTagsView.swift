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
        }
        .overlay { if tasks.allTags.isEmpty { Text("No Tags").foregroundStyle(.secondary) } }
        .navigationTitle("Manage Tags")
    }

    /// The picker's color: the Tag Color, or the grey of Default.
    private func color(of tag: TagEntry) -> Binding<Color> {
        Binding(
            get: { tag.color.flatMap(Color.init(tagColor:)) ?? .gray },
            set: { tasks.setTagColor(tag.name, to: $0.tagColor) })
    }
}
