import Foundation

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

// Translations from platform input types to the core's. Units and coordinate
// systems only; what an input means is the core's business (ADR-0006).

extension Viewport {
    /// The core's viewport (`Float` is plenty for drawing).
    init(_ state: ViewportState) {
        self.init(origin: [Float(state.originX), Float(state.originY)], zoom: Float(state.zoom))
    }
}

#if os(macOS)
    extension Modifiers {
        init(_ flags: NSEvent.ModifierFlags) {
            self.init(
                shift: flags.contains(.shift), ctrl: flags.contains(.control),
                alt: flags.contains(.option), meta: flags.contains(.command))
        }
    }
#else
    extension Modifiers {
        init(_ flags: UIKeyModifierFlags) {
            self.init(
                shift: flags.contains(.shift), ctrl: flags.contains(.control),
                alt: flags.contains(.alternate), meta: flags.contains(.command))
        }
    }

    extension PointerKind {
        init(_ type: UITouch.TouchType) {
            switch type {
            case .pencil: self = .pencil
            case .indirectPointer: self = .mouse
            default: self = .finger
            }
        }
    }
#endif
