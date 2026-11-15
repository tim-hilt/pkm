import Foundation

/// What a `pkm://` link does to the window: leaves what is open, switches to the
/// Context and opens the Item it names. A link to an Item that isn't there
/// leaves its Context's Gallery shown, or for Tasks the Active list.
@MainActor
struct AppLinks {
    let navigation: AppNavigation
    let gallery: Gallery
    let library: LibraryModel
    let tasks: TasksModel

    func follow(_ url: URL) {
        guard let link = navigation.follow(url) else { return }
        gallery.close()
        library.closeBook()
        tasks.openLinked(id: nil)
        guard let itemID = link.itemID else { return }
        switch link.context {
        case .drawings: gallery.openDrawing(linkedItemID: itemID)
        case .books: library.openBook(id: itemID)
        case .tasks: tasks.openLinked(id: itemID)
        case .notes, .bookmarks: break
        }
    }
}
