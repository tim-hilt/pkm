import SwiftUI

/// The single window: the five Contexts as tabs (a sidebar on a Mac, and on an
/// iPad when expanded), or, while a Drawing or a Book is open, that alone.
struct ContentView: View {
    @State private var gallery = Gallery.forLaunch()
    @State private var library = LibraryModel.forLaunch()
    @Environment(AppNavigation.self) private var navigation
    @Environment(CloudBackingModel.self) private var cloud
    @Environment(\.scenePhase) private var scenePhase
    #if os(macOS)
        @Environment(\.openSettings) private var openSettings
    #endif

    var body: some View {
        @Bindable var navigation = navigation
        Group {
            if let drawing = gallery.open {
                DrawingView(
                    drawing: drawing, export: ExportedDrawing(name: drawing.name, gallery: gallery),
                    onBack: gallery.close
                )
                // A fresh view per Drawing: its geometry is requested once.
                .id(drawing.name)
            } else if let book = library.open {
                BookStubView(book: book, onBack: library.closeBook)
            } else {
                TabView(selection: $navigation.selected) {
                    ForEach(AppContext.allCases) { context in
                        Tab(value: context) {
                            switch context {
                            case .drawings: GalleryView(gallery: gallery)
                            case .books: BooksView()
                            default: ComingSoonView(context: context)
                            }
                        } label: {
                            context.label
                        }
                    }
                }
                .tabViewStyle(.sidebarAdaptable)
            }
        }
        .environment(library)
        #if os(iOS)
            .sheet(isPresented: $navigation.isShowingSettings) { SettingsSheet() }
        #endif
        .onOpenURL { follow($0) }
        .task { attachCloudBacking() }
        .onChange(of: gallery.open != nil || library.open != nil, initial: true) {
            navigation.isItemOpen = gallery.open != nil || library.open != nil
        }
        .onChange(of: cloud.progress, initial: true) { gallery.syncProgress = cloud.progress }
        .onChange(of: cloud.status, initial: true) { gallery.syncStatus = cloud.status }
        .onChange(of: scenePhase) { cloud.appBecame(inFront: scenePhase == .active) }
        #if os(macOS)
            .task { if AppNavigation.opensSettingsAtLaunch { openSettings() } }
        #endif
    }
}

extension ContentView {
    /// Shows what a `pkm://` link names, leaving any open Drawing; a link
    /// that names no Context changes nothing.
    private func follow(_ url: URL) {
        guard let link = navigation.follow(url) else { return }
        gallery.close()
        library.closeBook()
        guard link.context == .drawings, let id = link.itemID else { return }
        gallery.openDrawing(linkedItemID: id)
    }

    /// Lets Cloud Backing Sync the Gallery's Drawings, and the Gallery tell it
    /// of changes. Not for the temporary stores of UI tests and screenshots.
    private func attachCloudBacking() {
        guard let stateDirectory = gallery.syncStateDirectory else { return }
        gallery.onLocalChange = { [cloud] in cloud.drawingsChanged() }
        cloud.startSyncing(
            drawings: gallery.drawingsDirectory, stateDirectory: stateDirectory,
            onChanged: { [gallery] names in gallery.syncDidChange(names) })
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
    /// `-UITestingOpenBooks`) and leave the Device's last AppContext alone.
    @MainActor
    static func forLaunch() -> AppNavigation {
        guard ProcessInfo.processInfo.arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return AppNavigation()
        }
        let navigation = AppNavigation(remembering: nil)
        let arguments = ProcessInfo.processInfo.arguments
        if opensSettingsAtLaunch || arguments.contains("-UITestingOpenSampleDrawing") {
            navigation.selected = .drawings
        } else if arguments.contains("-UITestingOpenBooks") {
            navigation.selected = .books
        }
        navigation.isShowingSettings = opensSettingsAtLaunch
        return navigation
    }
}
