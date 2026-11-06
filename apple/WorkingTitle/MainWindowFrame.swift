#if os(macOS)
    import AppKit
    import SwiftUI

    /// The Mac window's size on first launch.
    ///
    /// Why not `.defaultSize`: a SwiftUI `Window` here opens at 900 × 450 pt
    /// whatever `.defaultSize`, `.windowResizability` or an ideal content size
    /// say (the Canvas is an `NSViewRepresentable` and gives no ideal size;
    /// only a minimum content size is honoured, which would stop the window
    /// shrinking to the phone layout). 450 pt is under the 500 pt height at
    /// which `DrawingLayout` switches to the phone layout, so we set the frame
    /// ourselves, once, when the app has no saved frame. SwiftUI autosaves the
    /// frame under `savedFrameKey` afterwards and restores it on later launches.
    enum MainWindowFrame {
        static let savedFrameKey = "NSWindow Frame main"
        static let preferredSize = CGSize(width: 1280, height: 800)

        /// Whether the user has never had a window frame saved. Read before the
        /// window exists, as SwiftUI saves the frame it opens with.
        static func isFirstLaunch(defaults: UserDefaults = .standard) -> Bool {
            defaults.string(forKey: savedFrameKey) == nil
        }

        /// 1280 × 800 pt, or as large as `visible` allows, centred in it.
        static func initial(in visible: CGRect) -> CGRect {
            let size = CGSize(
                width: min(preferredSize.width, visible.width), height: min(preferredSize.height, visible.height))
            return CGRect(
                x: visible.midX - size.width / 2, y: visible.midY - size.height / 2, width: size.width,
                height: size.height)
        }

        /// Decided once per process, before SwiftUI opens the window.
        static let applies = isFirstLaunch()
    }

    extension View {
        /// Sizes the hosting window for the desktop layout on first launch.
        func firstLaunchWindowSize() -> some View {
            background(WindowSizer())
        }
    }

    private struct WindowSizer: NSViewRepresentable {
        final class Coordinator { var done = false }

        func makeCoordinator() -> Coordinator { Coordinator() }
        func makeNSView(context: Context) -> NSView { NSView() }

        func updateNSView(_ view: NSView, context: Context) {
            guard MainWindowFrame.applies, !context.coordinator.done else { return }
            DispatchQueue.main.async {
                guard !context.coordinator.done, let window = view.window, let screen = window.screen else { return }
                context.coordinator.done = true
                window.setFrame(MainWindowFrame.initial(in: screen.visibleFrame), display: true)
            }
        }
    }
#endif
