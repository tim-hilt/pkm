import Foundation
import Testing

@testable import pkm

/// Keeping the Show dimensions setting between launches: `CanvasController`
/// stores the user's choice and restores it into every editor. A new
/// controller and editor on the same settings stand for the next launch. What
/// the Dimensions show is tested in the core.
@MainActor @Suite struct ShowDimensionsTests {
    @MainActor private struct Launch {
        let editor = Editor()
        let controller: CanvasController

        init(settings: UserDefaults) {
            controller = CanvasController(settings: settings)
            let canvas = CanvasMetalView()
            canvas.editor = editor
            controller.canvas = canvas
        }
    }

    @Test func isOffByDefault() {
        let launch = Launch(settings: UserDefaults.isolated())

        #expect(!launch.controller.showDimensions)
        #expect(!launch.editor.showDimensions())
    }

    @Test func theChoiceIsKeptForTheNextLaunch() {
        let settings = UserDefaults.isolated()
        let first = Launch(settings: settings)

        first.controller.setShowDimensions(true)
        #expect(first.controller.showDimensions)

        let next = Launch(settings: settings)
        #expect(next.controller.showDimensions)
        #expect(next.editor.showDimensions())
    }

    @Test func switchingItOffIsKept() {
        let settings = UserDefaults.isolated()
        Launch(settings: settings).controller.setShowDimensions(true)
        Launch(settings: settings).controller.setShowDimensions(false)

        #expect(!Launch(settings: settings).controller.showDimensions)
    }
}
