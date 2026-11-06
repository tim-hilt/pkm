import SwiftUI

/// The single window: the Gallery, or the open Drawing in the Canvas.
struct ContentView: View {
    @State private var gallery = Gallery.forLaunch()
    @Environment(CloudBackingModel.self) private var cloud
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
            } else {
                GalleryView(gallery: gallery)
            }
        }
        .task { attachCloudBacking() }
        .onChange(of: scenePhase) { cloud.appBecame(inFront: scenePhase == .active) }
        #if os(macOS)
            .task { if Gallery.opensSettingsAtLaunch { openSettings() } }
        #endif
    }
}

extension ContentView {
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

extension DrawingStore {
    /// The app's Documents folder, which Files and Finder show, with the
    /// caches in Caches. UI tests ask for empty temporary folders instead.
    static var appDrawings: DrawingStore {
        appDrawings(temporary: ProcessInfo.processInfo.arguments.contains("-UITestingTemporaryStore"))
    }

    static func appDrawings(temporary: Bool) -> DrawingStore {
        if temporary {
            let root = FileManager.default.temporaryDirectory
                .appending(path: "ExcalidrawNativeUITests-\(UUID().uuidString)", directoryHint: .isDirectory)
            let directory = root.appending(path: "Documents", directoryHint: .isDirectory)
            try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            return DrawingStore(directory: directory, cacheDirectory: root.appending(path: "Caches"))
        }
        return DrawingStore(
            directory: .documentsDirectory,
            cacheDirectory: .cachesDirectory.appending(path: "Drawings", directoryHint: .isDirectory))
    }
}

extension Gallery {
    /// `-UITestingOpenSettings` (used by `make screenshot SCREEN=settings`)
    /// shows Settings over an empty temporary Gallery.
    static var opensSettingsAtLaunch: Bool {
        ProcessInfo.processInfo.arguments.contains("-UITestingOpenSettings")
    }

    /// The Gallery for this launch. `-UITestingOpenSampleDrawing` (used by
    /// `make screenshot`) starts in an empty temporary store with the sample
    /// Drawing already open, so agents can see the Canvas without tapping.
    @MainActor
    static func forLaunch() -> Gallery {
        let arguments = ProcessInfo.processInfo.arguments
        if opensSettingsAtLaunch { return Gallery(store: .appDrawings(temporary: true)) }
        guard arguments.contains("-UITestingOpenSampleDrawing") else { return Gallery(store: .appDrawings) }
        let store = DrawingStore.appDrawings(temporary: true)
        let name = try? store.add(Data(contentsOf: SampleDrawing.url), named: "sample")
        let gallery = Gallery(store: store)
        if let name { gallery.openDrawing(named: name) }
        return gallery
    }
}
