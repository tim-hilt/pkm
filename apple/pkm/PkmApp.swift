import SwiftUI

@main
struct PkmApp: App {
    #if os(macOS)
        init() { _ = MainWindowFrame.applies }  // before the window saves its frame
    #endif

    @State private var cloud = CloudBackingModel()
    @State private var navigation = AppNavigation.forLaunch()
    @State private var readerSettings = ReaderSettings.forLaunch()
    @State private var library = LibraryModel.forLaunch()
    @State private var tasks = TasksModel.forLaunch()
    @State private var notes = NotesModel.forLaunch()
    @State private var bookmarks = BookmarksModel.forLaunch()

    var body: some Scene {
        // One window (spec: Gallery → Canvas → back). A second window would
        // have its own Gallery, so the same Drawing could be open twice and
        // the two autosaves would overwrite each other's changes. iOS and
        // iPadOS get one window as the app does not declare multiple scenes.
        #if os(macOS)
            Window(Text(verbatim: "pkm"), id: "main") {
                ContentView().firstLaunchWindowSize().environment(cloud).environment(navigation)
                    .environment(readerSettings).environment(library).environment(tasks).environment(notes)
                    .environment(bookmarks).appliesAppearance()
            }
            .commands {
                GoCommands(navigation: navigation)
                ReaderCommands(settings: readerSettings)
                CanvasCommands()
                ImportCommands()
                AppearanceCommands()
                FormatCommands()
            }
            // ⌘, opens it.
            Settings {
                SettingsView().environment(cloud).environment(readerSettings).environment(library).environment(tasks)
                    .environment(notes)
            }
        #else
            WindowGroup {
                ContentView().environment(cloud).environment(navigation).environment(readerSettings)
                    .environment(library).environment(tasks).environment(notes).environment(bookmarks)
                    .appliesAppearance()
            }
            .commands {
                CanvasCommands()
                FormatCommands()
            }
        #endif
    }
}
