#if !os(macOS)
    import UIKit

    /// Lets the system's undo gestures (three-finger swipe left and right,
    /// three-finger double-tap, shake) drive the core's history. UIKit sends
    /// them to the first responder's undo manager; this one registers nothing
    /// and forwards to the `CanvasController`'s commands instead, since the
    /// core owns the history (ADR-0006).
    @MainActor
    final class CanvasUndoManager: UndoManager {
        private let canvas: () -> CanvasMetalView?

        init(canvas: @escaping () -> CanvasMetalView?) {
            self.canvas = canvas
            super.init()
        }

        override var canUndo: Bool { canvas()?.editor?.canUndo() ?? false }
        override var canRedo: Bool { canvas()?.editor?.canRedo() ?? false }
        override func undo() { canvas()?.perform { $0.undo() } }
        override func redo() { canvas()?.perform { $0.redo() } }
    }
#endif
