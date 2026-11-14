import Foundation

extension TasksModel {
    /// The Tasks folder in the app's Documents, which Files and Finder show.
    static let documentsFolder = URL.documentsDirectory.appending(path: "Tasks", directoryHint: .isDirectory)

    /// Where the Device ID lives: on this Device only, outside every Synced folder.
    static let deviceFolder = URL.applicationSupportDirectory.appending(path: "Device", directoryHint: .isDirectory)

    /// The Tasks for this launch: the Tasks folder. For UI tests and screenshots
    /// (`-UITesting…`) it is empty and temporary, with sample Tasks when
    /// launched with `-UITestingOpenTasks`.
    @MainActor
    static func forLaunch() -> TasksModel {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return TasksModel(store: openOrTemporary(documentsFolder))
        }
        let store = openOrTemporary(temporaryFolder())
        if arguments.contains("-UITestingOpenTasks") {
            let samples = [
                (
                    "Buy milk", "Two litres of oat milk, and a loaf of bread if the bakery is still open.", UInt8(1),
                    ["home", "shopping"]
                ),
                ("Book the dentist", "", 2, ["health"]),
                ("Reply to Anna's email", "About the trip", 3, []),
            ]
            for (title, description, priority, tags) in samples {
                if let task = try? store.create(title: title, planning: .active, priority: 4, tags: []) {
                    _ = try? store.edit(
                        id: task.id, title: title, description: description, priority: priority, addTags: [])
                    for tag in tags { _ = try? store.addTag(id: task.id, tag: tag) }
                    if title == "Buy milk" {
                        _ = try? store.createSubtask(parent: task.id, title: "Find the oat milk", priority: 4, tags: [])
                    }
                }
            }
            _ = try? store.create(title: "Learn Rust", planning: .backlog, priority: 4, tags: [])
        }
        return TasksModel(store: store)
    }

    private static func temporaryFolder() -> URL {
        FileManager.default.temporaryDirectory.appending(
            path: "pkmUITests-\(UUID().uuidString)/Tasks", directoryHint: .isDirectory)
    }

    /// The store in `folder`, created if needed. Where that fails, a temporary one:
    /// the app still runs, its Tasks just don't stay.
    private static func openOrTemporary(_ folder: URL) -> TaskStore {
        (try? TaskStore.open(root: folder.path, deviceFolder: deviceFolder.path))
            ?? (try! TaskStore.open(root: temporaryFolder().path, deviceFolder: temporaryFolder().path))
    }
}
