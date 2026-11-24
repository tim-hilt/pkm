import SwiftUI

extension TasksModel {
    /// The name of a bar entry: a Filter's Title, or "Filters".
    func title(of entry: TasksList) -> String {
        switch entry {
        case .filters: String(localized: "Filters")
        case .filter(let id): filters.first { $0.id == id }?.title ?? ""
        }
    }
}

/// Picks the list shown from the bar's entries: the Favorites, then Filters.
private struct TasksListPicker: View {
    @Environment(TasksModel.self) private var model

    var body: some View {
        Picker("List", selection: Binding(get: { model.barEntry }, set: { model.list = $0 })) {
            ForEach(model.bar.filter { $0 != .filters }, id: \.self) { Text(verbatim: model.title(of: $0)).tag($0) }
            #if os(iOS)
                Divider()
            #endif
            Text("Filters").tag(TasksList.filters)
        }
        .accessibilityIdentifier("tasks-lists")
    }
}

#if os(iOS)
    extension View {
        /// The shown list's name as the title; tapping it opens a menu of the Favorites in Filter
        /// order, a divider and "Filters". Inline: the menu opens from an inline title,
        /// not from an expanded large one.
        func tasksTitleMenu(_ model: TasksModel) -> some View {
            navigationTitle(model.title(of: model.barEntry))
                .navigationBarTitleDisplayMode(.inline)
                .toolbarTitleMenu { TasksListPicker() }
        }
    }
#else
    /// The Mac's bar: a segmented control in the toolbar, whose overflow takes the entries.
    struct TasksBar: View {
        var body: some View {
            TasksListPicker().pickerStyle(.segmented).labelsHidden()
        }
    }
#endif
