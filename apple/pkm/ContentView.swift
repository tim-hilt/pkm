import SwiftUI

/// The single window: the five Contexts in a sidebar (on an iPhone a list the
/// sidebar button or an edge swipe reveals; on iPhone and iPad with Settings
/// pinned to its bottom), or, while a Drawing or a Book is open, that alone.
struct ContentView: View {
    @State private var gallery = Gallery.forLaunch()
    @Environment(AppNavigation.self) private var navigation
    @Environment(LibraryModel.self) private var library
    @Environment(CloudBackingModel.self) private var cloud
    @Environment(TasksModel.self) private var tasks
    @Environment(NotesModel.self) private var notes
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.scenePhase) private var scenePhase
    /// An iPhone opens into the detail; its sidebar is one step back.
    @State private var column = NavigationSplitViewColumn.detail
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
                NavigationSplitView(preferredCompactColumn: $column) {
                    sidebar
                } detail: {
                    // One stack for every destination: on an iPhone, a detail whose own
                    // stack replaced the last one's never appeared (no `task`, `onAppear`).
                    NavigationStack(path: path) { detail }
                }
            }
        }
        .onOpenURL { AppLinks(navigation: navigation, gallery: gallery, library: library, tasks: tasks).follow($0) }
        .task { attachCloudBacking() }
        .onChange(of: gallery.open != nil || library.open != nil, initial: true) {
            navigation.isItemOpen = gallery.open != nil || library.open != nil
        }
        .onChange(of: cloud.progress, initial: true) { gallery.syncProgress = cloud.progress }
        .onChange(of: cloud.status, initial: true) { gallery.syncStatus = cloud.status }
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
    private var sidebar: some View {
        List(
            selection: Binding(
                get: { navigation.destination },
                set: { destination in
                    // nil when an iPhone goes back to the list: it stays there.
                    guard let destination else { return }
                    navigation.destination = destination
                    column = .detail
                })
        ) {
            ForEach(AppContext.allCases) { context in
                context.label.tag(AppNavigation.Destination.context(context))
            }
        }
        .navigationTitle("pkm")
        #if os(iOS)
            .safeAreaInset(edge: .bottom) {
                Button {
                    navigation.destination = .settings
                    column = .detail
                } label: {
                    Label("Settings", systemImage: "gearshape")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding()
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(navigation.isShowingSettings ? Color.accentColor : .primary)
                .background(.bar)
                .accessibilityIdentifier("settings")
            }
        #endif
    }

    /// What is pushed in the detail: Tasks and, where the editor doesn't fit beside the list,
    /// Notes push by path, the others by link.
    private var path: Binding<[String]> {
        switch navigation.destination {
        case .context(.tasks):
            return Binding(get: { tasks.openFilterPath }, set: { tasks.openFilterPath = $0 })
        case .context(.notes) where NotesLayout.pushes(sizeClass):
            return Binding(get: { notes.openID.map { [$0] } ?? [] }, set: { notes.openID = $0.last })
        default:
            return .constant([])
        }
    }

    @ViewBuilder private var detail: some View {
        switch navigation.destination {
        case .settings: SettingsView()
        case .context(.drawings): GalleryView(gallery: gallery)
        case .context(.tasks): TasksView()
        case .context(.books): BooksView()
        case .context(.notes): NotesView()
        case .context(let context): ComingSoonView(context: context)
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
            tasks: TasksModel.documentsFolder,
            stateDirectory: stateDirectory,
            onChanged: { [gallery] names in gallery.syncDidChange(names) },
            onBooksChanged: { [library] _ in library.syncDidChange() },
            onTasksChanged: { [tasks] _ in tasks.syncDidChange() })
    }
}

extension AppNavigation {
    /// `-UITestingOpenSettings` (used by `make screenshot SCREEN=settings`)
    /// shows Settings over an empty temporary Gallery.
    static var opensSettingsAtLaunch: Bool {
        ProcessInfo.processInfo.arguments.contains("-UITestingOpenSettings")
    }

    /// The navigation for this launch. The `-UITesting…` launch arguments
    /// (temporary stores, `make screenshot`) start on Drawings (Books for
    /// `-UITestingOpenBooks` and `-UITestingOpenReader`, Tasks for `-UITestingOpenTasks`
    /// and `-UITestingOpenTaskSheet`, Notes for `-UITestingOpenNotes` and `-UITestingOpenNote`) and leave the Device's last AppContext alone.
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
        } else if arguments.contains("-UITestingOpenBooks") || arguments.contains("-UITestingOpenReader") {
            navigation.selected = .books
        }
        navigation.isShowingSettings = opensSettingsAtLaunch
        return navigation
    }
}
