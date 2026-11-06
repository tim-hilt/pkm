import SwiftUI

@main
struct ExcalidrawNativeApp: App {
    #if os(macOS)
        init() { _ = MainWindowFrame.applies }  // before the window saves its frame
    #endif

    @State private var cloud = CloudBackingModel()

    var body: some Scene {
        // One window (spec: Gallery → Canvas → back). A second window would
        // have its own Gallery, so the same Drawing could be open twice and
        // the two autosaves would overwrite each other's changes. iOS and
        // iPadOS get one window as the app does not declare multiple scenes.
        #if os(macOS)
            Window(Text(verbatim: "Excalidraw Native"), id: "main") {
                ContentView().firstLaunchWindowSize().environment(cloud)
            }
            .commands { CanvasCommands() }
            // ⌘, opens it.
            Settings { SettingsView().environment(cloud) }
        #else
            WindowGroup { ContentView().environment(cloud) }
                .commands { CanvasCommands() }
        #endif
    }
}
