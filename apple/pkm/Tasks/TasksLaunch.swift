import Foundation

extension TasksModel {
    /// The Tasks folder in the app's Documents, which Files and Finder show.
    static let documentsFolder = URL.documentsDirectory.appending(path: "Tasks", directoryHint: .isDirectory)

    /// The Tasks for this launch: the Tasks folder. For UI tests and screenshots
    /// (`-UITesting…`) it is empty and temporary, with sample Tasks and the
    /// Favorites All Tasks, Active and Backlog when
    /// launched with `-UITestingOpenTasks`, the sheet of "Buy milk" open with
    /// `-UITestingOpenTaskSheet`, and also a Filter, with the Filters list shown with
    /// `-UITestingOpenFilters` and the Filter itself with `-UITestingOpenFilter`. With
    /// `-UITestingOpenGroupedFilter` the Filter shown is "Projects", Grouped by `@home` and `@backlog`
    /// (which its Query leaves empty).
    @MainActor
    static func forLaunch() -> TasksModel {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return TasksModel(store: openOrTemporary(documentsFolder), defaults: .standard)
        }
        let store = openOrTemporary(temporaryFolder())
        let opensSheet = arguments.contains("-UITestingOpenTaskSheet")
        let opensFilters = arguments.contains("-UITestingOpenFilters")
        let opensFilter = arguments.contains("-UITestingOpenFilter")
        let opensGrouped = arguments.contains("-UITestingOpenGroupedFilter")
        if opensSheet || opensFilters || opensFilter || opensGrouped || arguments.contains("-UITestingOpenTasks") {
            for (title, query) in [("All Tasks", "*"), ("Active", "!@backlog"), ("Backlog", "@backlog")] {
                guard let filter = try? store.createFilter(title: title, query: query, groupBy: "") else { continue }
                try? store.setFavorite(id: filter.id, favorite: true)
            }
            let samples = [
                (
                    "Buy milk", "Two litres of oat milk, and a loaf of bread if the bakery is still open.", UInt8(1),
                    ["home", "shopping"]
                ),
                ("Book the dentist", "", 2, ["health"]),
                ("Reply to Anna's email", "About the trip", 3, []),
            ]
            for (title, description, priority, tags) in samples {
                if let task = try? store.create(title: title, priority: 4, tags: []) {
                    _ = try? store.edit(
                        id: task.id, title: title, description: description, priority: priority, addTags: [])
                    for tag in tags { _ = try? store.addTag(id: task.id, tag: tag) }
                    if title == "Buy milk" {
                        _ = try? store.createSubtask(parent: task.id, title: "Find the oat milk", priority: 4, tags: [])
                    }
                }
            }
            _ = try? store.create(title: "Learn Rust", priority: 4, tags: ["backlog"])
            if opensFilters || opensFilter {
                // Favorites enough to overflow the bar on an iPhone.
                for (title, query) in [
                    ("Home and health", "@home | @health"), ("Shopping", "@shopping"), ("Work", "@work"),
                    ("Errands", "@errands"), ("Someday", "@someday"),
                ] {
                    guard let filter = try? store.createFilter(title: title, query: query, groupBy: "") else {
                        continue
                    }
                    try? store.setFavorite(id: filter.id, favorite: true)
                }
            }
        }
        var grouped: Filter?
        if opensGrouped {
            grouped = try? store.createFilter(title: "Projects", query: "!@backlog", groupBy: "@home @backlog")
        }
        let model = TasksModel(store: store)
        if let grouped { model.list = .filter(grouped.id) }
        if opensFilters { model.list = .filters }
        if opensFilter, let filter = model.filters.first { model.list = .filter(filter.id) }
        if opensSheet, let task = model.tasks.first { model.startEditing(task) }
        return model
    }

    private static func temporaryFolder() -> URL {
        FileManager.default.temporaryDirectory.appending(
            path: "pkmUITests-\(UUID().uuidString)/Tasks", directoryHint: .isDirectory)
    }

    /// The store in `folder`, created if needed. Where that fails, a temporary one:
    /// the app still runs, its Tasks just don't stay.
    private static func openOrTemporary(_ folder: URL) -> TaskStore {
        (try? TaskStore.open(root: folder.path))
            ?? (try! TaskStore.open(root: temporaryFolder().path))
    }
}
