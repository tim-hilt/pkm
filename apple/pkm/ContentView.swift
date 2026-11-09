import SwiftUI

/// The single window: the five Contexts as tabs (a sidebar on a Mac, and on an
/// iPad when expanded), or, while a Drawing is open, the Canvas alone.
struct ContentView: View {
    @State private var gallery = Gallery.forLaunch()
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
            } else {
                TabView(selection: $navigation.selected) {
                    ForEach(AppContext.allCases) { context in
                        Tab(value: context) {
                            if context == .drawings {
                                GalleryView(gallery: gallery)
                            } else {
                                ComingSoonView(context: context)
                            }
                        } label: {
                            context.label
                        }
                    }
                }
                .tabViewStyle(.sidebarAdaptable)
            }
        }
        #if os(iOS)
            .sheet(isPresented: $navigation.isShowingSettings) { SettingsSheet() }
        #endif
        .onOpenURL { follow($0) }
        .task { attachCloudBacking() }
        .onChange(of: gallery.open != nil, initial: true) { navigation.isItemOpen = gallery.open != nil }
        .onChange(of: cloud.connection != nil, initial: true) { gallery.isCloudBacked = cloud.connection != nil }
        .onChange(of: cloud.progress, initial: true) { gallery.syncProgress = cloud.progress }
        .onChange(of: cloud.status, initial: true) { gallery.syncStatus = cloud.status }
        .onChange(of: scenePhase) { cloud.appBecame(inFront: scenePhase == .active) }
        #if os(macOS)
            .task { if AppNavigation.opensSettingsAtLaunch { openSettings() } }
        #endif
    }
}

extension ContentView {
    /// Leaves any open Drawing, then shows what a `pkm://` link names.
    private func follow(_ url: URL) {
        gallery.close()
        guard let link = navigation.follow(url), link.context == .drawings, let id = link.itemID else { return }
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
    /// (temporary stores, `make screenshot`) start on Drawings and leave the
    /// Device's last AppContext alone.
    @MainActor
    static func forLaunch() -> AppNavigation {
        guard ProcessInfo.processInfo.arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return AppNavigation()
        }
        let navigation = AppNavigation(remembering: nil)
        if opensSettingsAtLaunch || ProcessInfo.processInfo.arguments.contains("-UITestingOpenSampleDrawing") {
            navigation.selected = .drawings
        }
        navigation.isShowingSettings = opensSettingsAtLaunch
        return navigation
    }
}
