import CoreGraphics

/// Keeps the text being typed visible above the on-screen keyboard: given
/// where the keyboard covers the Canvas and where the text and its caret are,
/// says how the core's viewport must move (the Scene and the Text view then
/// move together).
enum KeyboardAvoidance {
    /// Room kept between the caret's line and the keyboard or the Canvas's
    /// edge, in points.
    static let margin: CGFloat = 16

    /// The viewport that moves `text` to the middle of the room between `top`
    /// (the top of the screen) and `keyboard`, or the caret's line if the
    /// text is too tall for it. `nil` if the caret's line is clear of the
    /// keyboard already, or if no keyboard covers it (a hardware keyboard, or
    /// a floating one beside or above it). All rects are in the Canvas's
    /// coordinates; only panning, never zooming.
    static func viewport(
        keyboard: CGRect?, top: CGFloat, text: CGRect, caret: CGRect, current: ViewportState
    ) -> ViewportState? {
        guard let keyboard, current.zoom > 0 else { return nil }
        let line = CGRect(x: caret.minX, y: caret.minY, width: max(caret.width, 1), height: caret.height + margin)
        guard line.intersects(keyboard) else { return nil }
        let room = keyboard.minY - top
        let shown = text.height + 2 * margin <= room ? text : caret
        let lift = shown.midY - (top + room / 2)
        guard lift > 0 else { return nil }
        var panned = current
        panned.originY += Double(lift) / current.zoom
        return panned
    }

    /// Follows the caret out of view as Excalidraw does: the viewport that
    /// pans `caret` just into `room` (what is visible of the Canvas), with the
    /// margin to spare, or `nil` if it is in already. Only panning, never
    /// zooming.
    static func follow(caret: CGRect, into room: CGRect, current: ViewportState) -> ViewportState? {
        guard current.zoom > 0 else { return nil }
        let dx = overflow(caret.minX, caret.maxX, room.minX, room.maxX)
        let dy = overflow(caret.minY, caret.maxY, room.minY, room.maxY)
        guard dx != 0 || dy != 0 else { return nil }
        var panned = current
        panned.originX += Double(dx) / current.zoom
        panned.originY += Double(dy) / current.zoom
        return panned
    }

    /// How far `from...to` must move to be within `min...max`, the margin in.
    private static func overflow(_ from: CGFloat, _ to: CGFloat, _ min: CGFloat, _ max: CGFloat) -> CGFloat {
        if to > max - margin { return to - (max - margin) }
        if from < min + margin { return from - (min + margin) }
        return 0
    }
}
