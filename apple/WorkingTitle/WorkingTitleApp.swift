import SwiftUI

@main
struct WorkingTitleApp: App {
    var body: some Scene {
        WindowGroup { ContentView() }
            .commands { CanvasCommands() }
    }
}
