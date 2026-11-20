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
    }
#endif
