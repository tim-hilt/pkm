import CoreGraphics
import Testing

@testable import WorkingTitle

/// Which layout a Drawing's container size gets, following excalidraw.com's
/// form factors (docs/layout-reference §1): phone sizes get the mobile layout,
/// everything else the desktop one.
@Suite struct DrawingLayoutTests {
    @Test(arguments: [
        CGSize(width: 1470, height: 832),  // MacBook Air window
        CGSize(width: 1024, height: 768),  // small Mac window
        CGSize(width: 1024, height: 1366),  // iPad Pro portrait
        CGSize(width: 1366, height: 1024),  // iPad Pro landscape
        CGSize(width: 678, height: 1024),  // iPad Split View half
    ])
    func desktopSizesGetTheDesktopLayout(size: CGSize) {
        #expect(DrawingLayout(containerSize: size) == .desktop)
    }

    @Test(arguments: [
        CGSize(width: 393, height: 852),  // iPhone portrait
        CGSize(width: 852, height: 393),  // iPhone landscape
        CGSize(width: 599, height: 1024),  // narrow Split View
    ])
    func phoneSizesGetTheMobileLayout(size: CGSize) {
        #expect(DrawingLayout(containerSize: size) == .mobile)
    }

    @Test func theDesktopPropertiesPanelLeavesRoomForTheControls() {
        #expect(DrawingLayout.propertiesMaxHeight(containerHeight: 832) == 666)
        #expect(DrawingLayout.propertiesMaxHeight(containerHeight: 100) == 0)
    }
}
