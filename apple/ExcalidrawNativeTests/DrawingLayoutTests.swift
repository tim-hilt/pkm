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

    /// The desktop toolbar's measured width is about 456 pt.
    @Test(arguments: [
        (1470.0, 16.0),  // MacBook Air window
        (1366.0, 16.0),  // iPad full screen
        (1024.0, 76.0),  // 1024 pt Mac window
        (678.0, 76.0),  // iPad Split View half
    ])
    func thePropertiesPanelSharesTheToolbarsTopWhenItFits(width: CGFloat, top: CGFloat) {
        #expect(DrawingLayout.propertiesTop(containerWidth: width, toolbarWidth: 456) == top)
    }

    @Test func thePanelsRoomGrowsWithItsHeightAboveTheDefaultTop() {
        #expect(DrawingLayout.propertiesMaxHeight(containerHeight: 832, top: 16) == 726)
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

/// The phone's custom popover (ADR-0013): centred on its anchor, 8 pt from the
/// container's edges, its bottom 12 pt above the bar it belongs to.
@Suite struct PopoverGeometryTests {
    private let portrait = CGSize(width: 393, height: 852)
    private let landscape = CGSize(width: 852, height: 393)
    private let content = CGSize(width: 130, height: 52)

    @Test func itIsCentredOnItsAnchorAndSitsTwelvePointsAboveTheBar() {
        let anchor = CGRect(x: 150, y: 760, width: 36, height: 36)
        let p = DrawingLayout.popoverPlacement(anchor: anchor, content: content, container: portrait, barTop: 750)
        #expect(p.frame.origin == CGPoint(x: 103, y: 750 - 12 - 52))
        #expect(p.pointerX == 168)
    }

    @Test func itIsClampedEightPointsFromTheLeftEdgeWithTheTipStillOnTheAnchor() {
        let anchor = CGRect(x: 10, y: 760, width: 36, height: 36)
        let p = DrawingLayout.popoverPlacement(anchor: anchor, content: content, container: portrait, barTop: 750)
        #expect(p.frame.minX == 8)
        #expect(p.pointerX == 28)
    }

    @Test func itIsClampedEightPointsFromTheRightEdgeWithTheTipStillOnTheAnchor() {
        let anchor = CGRect(x: 350, y: 760, width: 36, height: 36)
        let p = DrawingLayout.popoverPlacement(anchor: anchor, content: content, container: portrait, barTop: 750)
        #expect(p.frame.minX == 255)
        #expect(p.pointerX == 368)
    }

    @Test func aPopoverWiderThanTheContainerKeepsEightPointsFromBothEdges() {
        let anchor = CGRect(x: 150, y: 760, width: 36, height: 36)
        let wide = CGSize(width: 500, height: 52)
        let p = DrawingLayout.popoverPlacement(anchor: anchor, content: wide, container: portrait, barTop: 750)
        #expect(p.frame.minX == 8 && p.frame.width == 377)
    }

    @Test func itsMaxHeightIsTheRoomAboveTheBarLessItsGapAndTheTopMargin() {
        let anchor = CGRect(x: 150, y: 760, width: 36, height: 36)
        let tall = CGSize(width: 130, height: 2000)
        let up = DrawingLayout.popoverPlacement(anchor: anchor, content: tall, container: portrait, barTop: 750)
        #expect(up.maxHeight == 730)
        #expect(up.frame.minY == 8)
        let side = DrawingLayout.popoverPlacement(
            anchor: CGRect(x: 400, y: 340, width: 36, height: 36), content: tall, container: landscape, barTop: 330)
        #expect(side.maxHeight == 310)
        #expect(side.frame.minY == 8)
    }
}
