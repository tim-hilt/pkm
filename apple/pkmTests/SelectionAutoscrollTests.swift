#if canImport(UIKit)
    import CoreGraphics
    import Testing

    @testable import pkm

    struct SelectionAutoscrollTests {
        @Test func scrollsTowardTheEdgeTheFingerIsNear() {
            #expect(autoscrollDelta(y: 300, top: 100, bottom: 700) == 0)
            #expect(autoscrollDelta(y: 110, top: 100, bottom: 700) < 0)
            #expect(autoscrollDelta(y: 690, top: 100, bottom: 700) > 0)
        }

        @Test func scrollsFasterBeyondTheEdgeButCapped() {
            #expect(autoscrollDelta(y: 50, top: 100, bottom: 700) < autoscrollDelta(y: 130, top: 100, bottom: 700))
            #expect(autoscrollDelta(y: -500, top: 100, bottom: 700) == -24)
        }

        private let start = CGRect(x: 20, y: 100, width: 2, height: 20)
        private let end = CGRect(x: 200, y: 300, width: 2, height: 20)

        @Test func aTouchOnACaretGrabsItsHandle() {
            #expect(selectionHandle(at: CGPoint(x: 21, y: 85), start: start, end: end) == .start)
            #expect(selectionHandle(at: CGPoint(x: 205, y: 335), start: start, end: end) == .end)
        }

        @Test func aTouchAwayFromTheCaretsGrabsNoHandle() {
            #expect(selectionHandle(at: CGPoint(x: 100, y: 200), start: start, end: end) == nil)
        }

        @Test func aTouchNearBothCaretsGrabsTheNearerOne() {
            let close = CGRect(x: 40, y: 100, width: 2, height: 20)
            #expect(selectionHandle(at: CGPoint(x: 36, y: 110), start: start, end: close) == .end)
        }
    }
#endif
