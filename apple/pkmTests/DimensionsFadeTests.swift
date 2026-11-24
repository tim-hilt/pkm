import Foundation
import Testing

@testable import pkm

/// How the Dimensions of a resize fade out and are announced; the output
/// itself is the core's.
@MainActor @Suite struct DimensionsFadeTests {
    private func output(_ texts: String...) -> Dimensions {
        Dimensions(
            geometry: Editor().geometry(),
            labels: texts.map { DimensionLabel(text: $0, x: 0, y: 0, angle: 0) },
            color: 0, opacity: 0.6)
    }

    @Test func nothingIsShownBeforeTheFirstOutput() {
        var fade = DimensionsFade()
        #expect(fade.update(nil, at: 0) == nil)
        #expect(fade.visible(at: 0) == nil)
        #expect(!fade.isFading(at: 0))
    }

    @Test func anOutputIsShownAtFullStrengthAtOnce() {
        var fade = DimensionsFade()
        _ = fade.update(output("1.00", "2.00"), at: 5)
        #expect(fade.visible(at: 5)?.strength == 1)
        #expect(!fade.isFading(at: 6))
    }

    @Test func afterTheOutputGoesAwayTheLastOneFadesOutOverAboutFourTenthsOfASecond() throws {
        var fade = DimensionsFade()
        _ = fade.update(output("1.00", "2.00"), at: 0)
        _ = fade.update(nil, at: 10)

        let half = try #require(fade.visible(at: 10.2))
        #expect(half.dimensions.labels.map(\.text) == ["1.00", "2.00"])
        #expect(abs(half.strength - 0.5) < 0.001)
        #expect(fade.isFading(at: 10.2))
        #expect(fade.visible(at: 10.4) == nil)
        #expect(!fade.isFading(at: 10.4))
    }

    @Test func askingAgainDuringTheFadeDoesNotRestartIt() {
        var fade = DimensionsFade()
        _ = fade.update(output("1.00", "2.00"), at: 0)
        _ = fade.update(nil, at: 10)
        #expect(fade.update(nil, at: 10.3) == nil)
        #expect(fade.visible(at: 10.4) == nil)
    }

    @Test func aNewOutputDuringTheFadeReplacesItAtFullStrength() throws {
        var fade = DimensionsFade()
        _ = fade.update(output("1.00", "2.00"), at: 0)
        _ = fade.update(nil, at: 10)
        _ = fade.update(output("3.00", "4.00"), at: 10.3)

        let shown = try #require(fade.visible(at: 10.3))
        #expect(shown.strength == 1)
        #expect(shown.dimensions.labels.map(\.text) == ["3.00", "4.00"])
        #expect(fade.visible(at: 11)?.strength == 1)
    }

    @Test func theEndOfAResizeIsAnnouncedOnceWithTheLastNumbers() {
        var fade = DimensionsFade()
        #expect(fade.update(output("11.40", "10.90"), at: 0) == nil)
        #expect(fade.update(output("11.40", "10.90"), at: 1) == nil)
        #expect(fade.update(nil, at: 2) == "width 11.40, height 10.90")
        #expect(fade.update(nil, at: 3) == nil)
    }

    @Test func aLineIsAnnouncedByItsLength() {
        var fade = DimensionsFade()
        _ = fade.update(output("11.40"), at: 0)
        #expect(fade.update(nil, at: 1) == "length 11.40")
    }

    @Test func aResetForgetsTheDimensionsWithoutFadeOrAnnouncement() {
        var fade = DimensionsFade()
        _ = fade.update(output("1.00", "2.00"), at: 0)
        fade.reset()
        #expect(fade.visible(at: 0) == nil)
        #expect(fade.update(nil, at: 1) == nil)
    }
}
