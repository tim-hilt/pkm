import CoreGraphics
import SwiftUI

/// The platform's look of the core's `Cursor` (ADR-0006: the core decides
/// which cursor shows, the client only how it looks). A Mac has system
/// cursors for most; an iPad's pointer takes a shape instead.
///
/// The shapes are centred on the pointer's location, in points.
@MainActor
enum CursorShape {
    /// A double-headed arrow, horizontal before `angle` (clockwise, radians).
    static func doubleArrow(angle: CGFloat = 0) -> CGPath {
        let path = CGMutablePath()
        let half: CGFloat = 10
        let head: CGFloat = 4.5
        let wing: CGFloat = 3.5
        let shaft: CGFloat = 0.9
        path.addLines(between: [
            CGPoint(x: -half, y: 0),
            CGPoint(x: -half + head, y: -wing),
            CGPoint(x: -half + head, y: -shaft),
            CGPoint(x: half - head, y: -shaft),
            CGPoint(x: half - head, y: -wing),
            CGPoint(x: half, y: 0),
            CGPoint(x: half - head, y: wing),
            CGPoint(x: half - head, y: shaft),
            CGPoint(x: -half + head, y: shaft),
            CGPoint(x: -half + head, y: wing),
        ])
        path.closeSubpath()
        var turn = CGAffineTransform(rotationAngle: angle)
        return path.copy(using: &turn) ?? path
    }

    /// Arrows in all four directions: moving.
    static let move: CGPath = {
        let path = CGMutablePath()
        path.addPath(doubleArrow())
        path.addPath(doubleArrow(angle: .pi / 2))
        return path
    }()

    /// A thin cross: drawing.
    static let crosshair: CGPath = {
        let path = CGMutablePath()
        path.addRect(CGRect(x: -8, y: -0.75, width: 16, height: 1.5))
        path.addRect(CGRect(x: -0.75, y: -8, width: 1.5, height: 16))
        return path
    }()

    /// A small disc: erasing, like excalidraw.com's eraser cursor.
    static let eraser = CGPath(ellipseIn: CGRect(x: -5, y: -5, width: 10, height: 10), transform: nil)

    /// The angle of the resize arrows of `cursor`, if it is a resize cursor.
    static func resizeAngle(of cursor: Cursor) -> CGFloat? {
        switch cursor {
        case .resizeEastWest: 0
        case .resizeNorthWestSouthEast: .pi / 4
        case .resizeNorthSouth: .pi / 2
        case .resizeNorthEastSouthWest: 3 * .pi / 4
        default: nil
        }
    }
}

#if os(macOS)
    import AppKit

    @MainActor
    extension Cursor {
        /// The Mac's cursor: the system's where it has one.
        var nsCursor: NSCursor {
            switch self {
            case .arrow: .arrow
            case .crosshair: .crosshair
            case .text: .iBeam
            case .move: Self.moveCursor
            case .grab: .openHand
            case .grabbing: .closedHand
            case .pointer: .pointingHand
            case .eraser: Self.eraserCursor
            case .resizeNorthSouth: .frameResize(position: .top, directions: .all)
            case .resizeEastWest: .frameResize(position: .left, directions: .all)
            case .resizeNorthWestSouthEast: .frameResize(position: .topLeft, directions: .all)
            case .resizeNorthEastSouthWest: .frameResize(position: .topRight, directions: .all)
            }
        }

        /// The Mac has no public move cursor; the four arrows, black with a
        /// white outline like the system's cursors.
        private static let moveCursor = cursor(CursorShape.move, fill: .black, outline: .white)
        private static let eraserCursor = cursor(CursorShape.eraser, fill: .white, outline: .black)

        private static func cursor(_ shape: CGPath, fill: NSColor, outline: NSColor) -> NSCursor {
            let size: CGFloat = 26
            let image = NSImage(size: NSSize(width: size, height: size), flipped: true) { _ in
                guard let context = NSGraphicsContext.current?.cgContext else { return false }
                context.translateBy(x: size / 2, y: size / 2)
                context.setLineJoin(.round)
                context.addPath(shape)
                context.setStrokeColor(outline.cgColor)
                context.setLineWidth(2)
                context.strokePath()
                context.addPath(shape)
                context.setFillColor(fill.cgColor)
                context.fillPath()
                return true
            }
            return NSCursor(image: image, hotSpot: NSPoint(x: size / 2, y: size / 2))
        }
    }
#else
    import UIKit

    @MainActor
    extension Cursor {
        /// The iPad pointer's look; `nil` for the system's round pointer,
        /// which stands in for the arrow and the hands (iPadOS has no hand).
        var pointerStyle: UIPointerStyle? {
            let shape: UIPointerShape
            if let angle = CursorShape.resizeAngle(of: self) {
                shape = .path(UIBezierPath(cgPath: CursorShape.doubleArrow(angle: angle)))
            } else {
                switch self {
                case .text: shape = .verticalBeam(length: 20)
                case .crosshair: shape = .path(UIBezierPath(cgPath: CursorShape.crosshair))
                case .move: shape = .path(UIBezierPath(cgPath: CursorShape.move))
                case .eraser: shape = .path(UIBezierPath(cgPath: CursorShape.eraser))
                default: return nil
                }
            }
            return UIPointerStyle(shape: shape)
        }
    }
#endif

extension View {
    /// Shows over a button of the chrome that it can be clicked: the pointing
    /// hand on a Mac, the pointer taking the button's shape on an iPad.
    func buttonPointer() -> some View {
        #if os(macOS)
            pointerStyle(.link)
        #else
            hoverEffect(.highlight)
        #endif
    }
}
