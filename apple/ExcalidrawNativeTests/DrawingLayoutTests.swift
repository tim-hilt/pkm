import CoreGraphics
import Testing

@testable import ExcalidrawNative

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

/// excalidraw.com's phone chrome (docs/layout-reference §4): a bottom bar
/// whose toolbar folds Tool families into one slot each.
@Suite struct MobileLayoutTests {
    @Test func theBottomBarSpansTheWidthUpTo450() {
        #expect(DrawingLayout.bottomBarWidth(containerWidth: 393) == 365)
        #expect(DrawingLayout.bottomBarWidth(containerWidth: 852) == 450)
    }

    @Test func everyV1ToolHasASlotInTheMobileToolbar() {
        let tools: [Tool] = [
            .hand, .selection, .rectangle, .diamond, .ellipse, .arrow, .line, .freedraw, .text, .eraser,
        ]
        for tool in tools {
            #expect(MobileToolSlot.all.contains { $0.tools.contains(tool) })
        }
    }

    @Test func theSlotsAreExcalidrawsPhoneOrder() {
        #expect(
            MobileToolSlot.all.map(\.tools.first) == [
                .hand, .selection, .freedraw, .eraser, .rectangle, .arrow, .text,
            ])
    }

    @Test func aSlotShowsItsLastChosenToolOfTheFamily() {
        let shapes = MobileToolSlot.all.first { $0.tools.contains(.diamond) }!
        #expect(shapes.tools == [.rectangle, .diamond, .ellipse])
        #expect(shapes.shownTool(current: .diamond, last: .rectangle) == .diamond)
        #expect(shapes.shownTool(current: .hand, last: .ellipse) == .ellipse)
        #expect(shapes.shownTool(current: .hand, last: nil) == .rectangle)
    }
}
