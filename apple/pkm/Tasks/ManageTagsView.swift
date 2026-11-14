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

/// Every Tag, alphabetically, each `a/b` as one flat row; "No Tags" if there
/// are none.
struct ManageTagsView: View {
    @Environment(TasksModel.self) private var tasks

    var body: some View {
        let tags = tasks.allTags
        List(tags, id: \.self) { Text($0) }
            .overlay { if tags.isEmpty { Text("No Tags").foregroundStyle(.secondary) } }
            .navigationTitle("Manage Tags")
    }
}
