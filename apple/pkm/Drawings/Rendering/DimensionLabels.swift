import QuartzCore

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// The numbers of Dimensions over the Canvas: the system font at a constant
/// screen size, each centred on the Scene point the core gives and turned by
/// its angle. The lines and ticks are the renderer's (`setDimensions`).
@MainActor
final class DimensionLabels {
    /// Holds one text layer per label; add it to the Canvas's layer.
    let layer = CALayer()
    private static let fontSize: CGFloat = 11
    private var shown: Dimensions?

    /// Shows `dimensions`' labels in `viewport`; nil takes them away.
    func show(_ dimensions: Dimensions?, in viewport: Viewport) {
        shown = dimensions
        place(in: viewport)
    }

    /// Puts the labels where `viewport` has their anchors; pans and zooms
    /// move them.
    func place(in viewport: Viewport) {
        let labels = shown?.labels ?? []
        // Turning a layer's text off and on again would flash; only the
        // number of layers follows the labels.
        while (layer.sublayers?.count ?? 0) > labels.count { layer.sublayers?.last?.removeFromSuperlayer() }
        while (layer.sublayers?.count ?? 0) < labels.count { layer.addSublayer(CATextLayer()) }
        guard let shown, let texts = layer.sublayers else { return }
        let font = Self.font
        let color = Self.color(shown.color, opacity: shown.opacity)
        // Layer positions are in points, so that no disabled animation is needed.
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        for (label, text) in zip(labels, texts) {
            guard let text = text as? CATextLayer else { continue }
            let size = (label.text as NSString).size(withAttributes: [.font: font])
            text.string = label.text
            text.font = font
            text.fontSize = Self.fontSize
            text.foregroundColor = color
            text.alignmentMode = .center
            text.contentsScale = layer.superlayer?.contentsScale ?? 1
            text.bounds = CGRect(origin: .zero, size: CGSize(width: ceil(size.width), height: ceil(size.height)))
            text.position = CGPoint(
                x: CGFloat((Float(label.x) - viewport.origin.x) * viewport.zoom),
                y: CGFloat((Float(label.y) - viewport.origin.y) * viewport.zoom))
            text.transform = CATransform3DMakeRotation(CGFloat(label.angle), 0, 0, 1)
        }
        CATransaction.commit()
    }

    #if os(macOS)
        private static var font: NSFont { .systemFont(ofSize: fontSize) }
    #else
        private static var font: UIFont { .systemFont(ofSize: fontSize) }
    #endif

    /// `rgba` (`0xRRGGBBAA`) at `opacity`.
    private static func color(_ rgba: UInt32, opacity: Float) -> CGColor {
        let channel = { (shift: UInt32) in CGFloat((rgba >> shift) & 0xff) / 255 }
        return CGColor(red: channel(24), green: channel(16), blue: channel(8), alpha: channel(0) * CGFloat(opacity))
    }
}
