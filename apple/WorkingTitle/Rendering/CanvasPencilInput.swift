#if !os(macOS)
    import UIKit

    /// The Apple Pencil's double-tap (ADR-0006). The system setting for it
    /// (Settings > Apple Pencil) is forwarded to the core with the tap, which
    /// switches the Tool; this adapter decides nothing.
    extension CanvasMetalView: UIPencilInteractionDelegate {
        /// Makes the Canvas receive Pencil double-taps.
        func enablePencilInput() {
            addInteraction(UIPencilInteraction(delegate: self))
        }

        func pencilInteraction(_ interaction: UIPencilInteraction, didReceiveTap tap: UIPencilInteraction.Tap) {
            let action = PencilDoubleTap(UIPencilInteraction.preferredTapAction)
            perform { $0.pencilDoubleTap(action: action) }
        }
    }

    extension PencilDoubleTap {
        /// The settings that are not about Tools (colour palette, ink
        /// attributes, shortcuts) are ignored: the app has none of them.
        init(_ action: UIPencilPreferredAction) {
            switch action {
            case .switchEraser: self = .switchEraser
            case .switchPrevious: self = .switchPrevious
            default: self = .ignore
            }
        }
    }
#endif
