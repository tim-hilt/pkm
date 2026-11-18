import Foundation
import ImageIO
import UniformTypeIdentifiers

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
                    "Buy milk",
                    """
                    Two litres of oat milk, and a loaf of bread if the bakery is still open.

                    ## Shops
                    - The **corner** [bakery](https://example.com), for *bread*
                    - `Aldi` for the milk
                    1. Bakery
                    1. Aldi
                    > Closes at six
                    ```
                    list = ["milk", "bread"]
                    ```
                    """, UInt8(1), ["home", "shopping"]
                ),
                ("Book the dentist", "", 2, ["health"]),
                ("Reply to Anna's email", "About the trip", 3, []),
            ]
            for (title, var description, priority, tags) in samples {
                if title == "Buy milk", let reference = (try? store.addImage(bytes: sampleImage())) {
                    description += "\n![](\(reference))\nThe shopping list"
                }
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
            _ = try? store.create(title: "Try *italic*, **bold** and `code`", priority: 4, tags: ["backlog"])
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

    /// A PNG of a gradient, for a sample Description's Image.
    private static func sampleImage() -> Data {
        let (width, height) = (600, 200)
        let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        let colors = [
            CGColor(red: 0.2, green: 0.5, blue: 1, alpha: 1), CGColor(red: 1, green: 0.5, blue: 0.2, alpha: 1),
        ]
        context.drawLinearGradient(
            CGGradient(colorsSpace: nil, colors: colors as CFArray, locations: nil)!, start: .zero,
            end: CGPoint(x: width, y: height), options: [])
        let png = NSMutableData()
        let destination = CGImageDestinationCreateWithData(png, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, context.makeImage()!, nil)
        CGImageDestinationFinalize(destination)
        return png as Data
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
