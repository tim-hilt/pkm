import Foundation

/// What of the Dimensions is on screen when: the core's output goes away at
/// once, the client lets the last one fade out (the core has no timers).
/// Pure state over a time the caller passes in.
struct DimensionsFade {
    /// Seconds from the end of a resize until the Dimensions are gone.
    static let duration: TimeInterval = 0.4

    private var last: Dimensions?
    /// When the output went away; nil while it is there.
    private var fadeStart: TimeInterval?

    /// Takes the core's current output at `now`. Returns what VoiceOver should
    /// announce if that output just went away, i.e. a resize just ended.
    /// A new output replaces a fading one at full strength; none starts or
    /// continues the fade. Nothing fades in.
    mutating func update(_ output: Dimensions?, at now: TimeInterval) -> String? {
        if let output {
            last = output
            fadeStart = nil
            return nil
        }
        guard let last, fadeStart == nil else { return nil }
        fadeStart = now
        return Self.announcement(of: last)
    }

    /// Forgets everything, without fading: for a setting switched off.
    mutating func reset() {
        last = nil
        fadeStart = nil
    }

    /// The Dimensions to draw at `now` and their strength, 1 for full down to
    /// just above 0; nil once they have faded out.
    func visible(at now: TimeInterval) -> (dimensions: Dimensions, strength: Float)? {
        guard let last else { return nil }
        guard let fadeStart else { return (last, 1) }
        let left = 1 - (now - fadeStart) / Self.duration
        return left > 0 ? (last, Float(left)) : nil
    }

    /// Whether the picture changes with time at `now`.
    func isFading(at now: TimeInterval) -> Bool {
        fadeStart != nil && visible(at: now) != nil
    }

    /// "width 11.40, height 10.90" for the two numbers of a box, "length
    /// 11.40" for one.
    static func announcement(of dimensions: Dimensions) -> String? {
        let texts = dimensions.labels.map(\.text)
        switch texts.count {
        case 2: return String(localized: "width \(texts[0]), height \(texts[1])")
        case 1: return String(localized: "length \(texts[0])")
        default: return nil
        }
    }
}
