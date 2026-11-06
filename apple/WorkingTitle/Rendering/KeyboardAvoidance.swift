import CoreGraphics

/// Keeps the line being typed above the on-screen keyboard: given where the
/// keyboard covers the Canvas and where the caret is, says how the core's
/// viewport must move (the Scene and the Text view then move together).
/// Excalidraw scrolls its textarea into view in the same way.
enum KeyboardAvoidance {
    /// Room kept between the caret's line and the keyboard, in points.
    static let margin: CGFloat = 16

    /// The viewport that lifts `caret` clear of `keyboard`, or `nil` if it
    /// already is, or if no keyboard covers the Canvas (a hardware keyboard).
    /// Both rects are in the Canvas's coordinates; only panning, never zooming.
    static func viewport(keyboard: CGRect?, caret: CGRect, current: ViewportState) -> ViewportState? {
        guard let keyboard, !keyboard.isNull, !keyboard.isEmpty, current.zoom > 0 else { return nil }
        let lift = caret.maxY + margin - keyboard.minY
        guard lift > 0 else { return nil }
        var panned = current
        panned.originY += Double(lift) / current.zoom
        return panned
    }
}
