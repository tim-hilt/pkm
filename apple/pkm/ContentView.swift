import SwiftUI

/// The single window: the five Contexts as tabs (a tab bar on an iPhone, which
/// an iPad and a Mac can turn into a sidebar), each with its own stack of
/// pushed screens; or, while a Drawing or a Book is open, that alone. On
/// iPhone and iPad Settings is a sheet over the tabs.
struct ContentView: View {
    @State private var gallery = Gallery.forLaunch()
    @Environment(AppNavigation.self) private var navigation
    @Environment(LibraryModel.self) private var library
    @Environment(CloudBackingModel.self) private var cloud
    @Environment(TasksModel.self) private var tasks
    @Environment(NotesModel.self) private var notes
    @Environment(BookmarksModel.self) private var bookmarks
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.scenePhase) private var scenePhase
    #if os(macOS)
        @Environment(\.openSettings) private var openSettings
    #endif

    var body: some View {
        Group {
            if let drawing = gallery.open {
                DrawingView(
                    drawing: drawing, export: ExportedDrawing(name: drawing.name, gallery: gallery),
                    onBack: gallery.close
                )
                // A fresh view per Drawing: its geometry is requested once.
                .id(drawing.name)
            } else if let book = library.open {
                // A new Reader per Book, so that nothing of the previous one carries over.
                OpenBookView(book: book, onGoToGallery: library.closeBook).id(book.id)
            } else {
                tabs
            }
        }
        #if os(iOS)
            .sheet(isPresented: Bindable(navigation).isShowingSettings) { SettingsView() }
            // A sheet that is presented from the first frame is dropped; this one waits for the tabs.
            .task { navigation.isShowingSettings = AppNavigation.opensSettingsAtLaunch }
        #endif
        .onOpenURL {
            AppLinks(navigation: navigation, gallery: gallery, library: library, tasks: tasks, bookmarks: bookmarks)
                .follow($0)
        }
        .task { attachCloudBacking() }
        .onChange(of: gallery.open != nil || library.open != nil, initial: true) {
            navigation.isItemOpen = gallery.open != nil || library.open != nil
        }
        .onChange(of: cloud.progress, initial: true) { gallery.syncProgress = cloud.progress }
        .onChange(of: cloud.status, initial: true) { gallery.syncStatus = cloud.status }
        // Takes in what the Share extension Saved, and tells it what is Saved.
        .onChange(of: scenePhase, initial: true) {
            bookmarks.exchangeWithShareExtension(tags: tasks.allTags.map(\.name))
        }
        .onChange(of: scenePhase) {
            cloud.appBecame(inFront: scenePhase == .active)
            if scenePhase == .background { tasks.commit() }
        }
        #if os(macOS)
            .task { if AppNavigation.opensSettingsAtLaunch { openSettings() } }
            // A Mac app quits without going to the background first.
            .onReceive(NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)) { _ in
                tasks.commit()
            }
        #endif
    }
}

extension ContentView {
    private var tabs: some View {
        @Bindable var navigation = navigation
        return TabView(selection: $navigation.selected) {
            ForEach(AppContext.allCases) { context in
                Tab(value: context) {
                    NavigationStack(path: path(of: context)) { root(of: context).settingsButton() }
                } label: {
                    context.label
                }
            }
        }
        .tabViewStyle(.sidebarAdaptable)
    }

    /// What is pushed over the Context's first screen: Tasks and, where the editor doesn't fit
    /// beside the list, Notes push by path, Bookmarks too, the others push nothing.
    private func path(of context: AppContext) -> Binding<[String]> {
        switch context {
        case .tasks:
            return Binding(get: { tasks.openFilterPath }, set: { tasks.openFilterPath = $0 })
        case .notes where NotesLayout.pushes(sizeClass):
            return Binding(get: { notes.pushedPath }, set: { notes.pushedPath = $0 })
        case .bookmarks:
            return Binding(get: { bookmarks.pushedPath }, set: { bookmarks.pushedPath = $0 })
        default:
            return .constant([])
        }
    }

    @ViewBuilder private func root(of context: AppContext) -> some View {
        switch context {
        case .drawings: GalleryView(gallery: gallery)
        case .tasks: TasksView()
        case .books: BooksView()
        case .notes: NotesView()
        case .bookmarks: BookmarksView()
        }
    }

    /// Lets Cloud Backing Sync the Gallery's Drawings, and the Gallery tell it
    /// of changes. Not for the temporary stores of UI tests and screenshots.
    private func attachCloudBacking() {
        guard let stateDirectory = gallery.syncStateDirectory else { return }
        gallery.onLocalChange = { [cloud] in cloud.drawingsChanged() }
        cloud.startSyncing(
            drawings: gallery.drawingsDirectory,
            books: LibraryModel.documentsFolder,
            bookmarks: BookmarksModel.documentsFolder,
            tasks: TasksModel.documentsFolder,
            notes: NotesModel.documentsFolder,
            stateDirectory: stateDirectory,
            onChanged: { [gallery] names in gallery.syncDidChange(names) },
            onBooksChanged: { [library] _ in library.syncDidChange() },
            onBookmarksChanged: { [bookmarks] _ in bookmarks.syncDidChange() },
            onTasksChanged: { [tasks] _ in tasks.syncDidChange() },
            onNotesChanged: { [notes] _ in notes.syncDidChange() })
    }
}

extension AppNavigation {
    /// `-UITestingOpenSettings` (used by `make screenshot SCREEN=settings`)
    /// shows Settings (the sheet) over an empty temporary Gallery.
    static var opensSettingsAtLaunch: Bool {
        ProcessInfo.processInfo.arguments.contains("-UITestingOpenSettings")
    }

    /// The navigation for this launch. The `-UITesting…` launch arguments
    /// (temporary stores, `make screenshot`) start on Drawings (Books for
    /// `-UITestingOpenBooks` and `-UITestingOpenReader`, Tasks for `-UITestingOpenTasks`
    /// and `-UITestingOpenTaskSheet`, Notes for `-UITestingOpenNotes` and `-UITestingOpenNote`, Bookmarks for `-UITestingOpenBookmarks`,
    /// `-UITestingOpenArticle` and `-UITestingOpenBookmarkDetails`) and leave the Device's last AppContext alone.
    @MainActor
    static func forLaunch() -> AppNavigation {
        guard ProcessInfo.processInfo.arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return AppNavigation()
        }
        let navigation = AppNavigation(remembering: nil)
        let arguments = ProcessInfo.processInfo.arguments
        if opensSettingsAtLaunch || arguments.contains("-UITestingOpenSampleDrawing") {
            navigation.selected = .drawings
        } else if arguments.contains("-UITestingOpenTasks") || arguments.contains("-UITestingOpenTaskSheet") {
            navigation.selected = .tasks
        } else if arguments.contains("-UITestingOpenNotes") || arguments.contains("-UITestingOpenNote") {
            navigation.selected = .notes
        } else if [
            "-UITestingOpenBookmarks", "-UITestingOpenArticle", "-UITestingOpenBookmarkDetails",
            "-UITestingSearchBookmarks", "-UITestingFindInArticle",
        ].contains(where: arguments.contains) {
            navigation.selected = .bookmarks
        } else if arguments.contains("-UITestingOpenBooks") || arguments.contains("-UITestingOpenReader") {
            navigation.selected = .books
        }
        return navigation
    }
}
