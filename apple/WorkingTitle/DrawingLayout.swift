import CoreGraphics

/// Which of excalidraw.com's layouts a Drawing's chrome uses, decided from the
/// size of its container alone (docs/layout-reference §1), so that it follows
/// window resizing, Split View and Stage Manager. Excalidraw's in-between
/// "tablet" form factor gets the desktop layout: the spec has only two.
enum DrawingLayout: Equatable {
    /// Top toolbar, properties panel on the left.
    case desktop
    /// Bottom toolbar, properties in a sheet.
    case mobile

    init(containerSize size: CGSize) {
        let phone = size.width <= 599 || (size.height < 500 && size.width < 1000)
        self = phone ? .mobile : .desktop
    }

    /// Margin of every floating control from the container's edge.
    static let margin: CGFloat = 16
    /// Top of the desktop properties panel, below the menu button.
    static let propertiesTop: CGFloat = 76
    /// Excalidraw's `containerHeight − 166`: the desktop properties panel
    /// scrolls beyond this, clear of the zoom and undo controls.
    static func propertiesMaxHeight(containerHeight: CGFloat) -> CGFloat {
        max(0, containerHeight - 166)
    }
}
