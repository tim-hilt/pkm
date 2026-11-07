import CoreGraphics

/// Which of excalidraw.com's layouts a Drawing's chrome uses, decided from the
/// size of its container alone (docs/layout-reference §1), so that it follows
/// window resizing, Split View and Stage Manager. Excalidraw's in-between
/// "tablet" form factor gets the desktop layout: the spec has only two.
enum DrawingLayout: Equatable {
    /// Top toolbar, properties panel on the left.
    case desktop
    /// Bottom toolbar, properties in popovers.
    case mobile

    init(containerSize size: CGSize) {
        let phone = size.width <= 599 || (size.height < 500 && size.width < 1000)
        self = phone ? .mobile : .desktop
    }

    /// Margin of every floating control from the container's edge.
    static let margin: CGFloat = 16
    /// Top of the desktop properties panel, below the toolbar.
    static let propertiesTop: CGFloat = 76
    /// The desktop properties panel's width.
    static let propertiesWidth: CGFloat = 195
    /// Clear room kept between the panel and a toolbar that shares its top.
    static let propertiesGap: CGFloat = 80
    /// Top of the desktop properties panel: level with the toolbar when the
    /// centred toolbar leaves the panel (and a gap) clear on the left,
    /// otherwise below the toolbar.
    static func propertiesTop(containerWidth: CGFloat, toolbarWidth: CGFloat) -> CGFloat {
        let toolbarLeft = (containerWidth - toolbarWidth) / 2
        let fits = toolbarLeft >= margin + propertiesWidth + propertiesGap
        return fits ? margin : propertiesTop
    }
    /// Excalidraw's `containerHeight − 166`: the desktop properties panel
    /// scrolls beyond this, clear of the zoom controls. A panel sitting
    /// higher than `propertiesTop` (at `top`) gets that much more room.
    static func propertiesMaxHeight(containerHeight: CGFloat, top: CGFloat = propertiesTop) -> CGFloat {
        max(0, containerHeight - 166 + (propertiesTop - top))
    }
}

extension DrawingLayout {
    /// The mobile bottom bar (action row over the toolbar): Excalidraw's
    /// `min(100% − 28, 450)`, centred.
    static func bottomBarWidth(containerWidth: CGFloat) -> CGFloat {
        max(0, min(containerWidth - 28, 450))
    }
}

/// Where a phone popover sits (ADR-0013).
struct PopoverPlacement: Equatable {
    /// The popover's frame in the container: its content size, capped to the
    /// container's width less the edge margins and to `maxHeight`.
    let frame: CGRect
    /// The pointer tip's x in the container: always on the anchor's centre.
    let pointerX: CGFloat
    /// The tallest the popover may be; it scrolls beyond.
    let maxHeight: CGFloat
}

extension DrawingLayout {
    /// Least distance of a popover from the container's edges.
    static let popoverEdgeMargin: CGFloat = 8
    /// Gap between a popover's bottom and the bar it belongs to.
    static let popoverGap: CGFloat = 12

    /// A popover as wide as its content, centred on the anchor but kept
    /// `popoverEdgeMargin` from the container's edges, its bottom
    /// `popoverGap` above the bar. `barTop` is the bar's top in the container.
    static func popoverPlacement(
        anchor: CGRect, content: CGSize, container: CGSize, barTop: CGFloat
    ) -> PopoverPlacement {
        let maxHeight = max(0, barTop - popoverGap - popoverEdgeMargin)
        let height = min(content.height, maxHeight)
        let widest = max(0, container.width - 2 * popoverEdgeMargin)
        let width = min(content.width, widest)
        let x = min(max(anchor.midX - width / 2, popoverEdgeMargin), popoverEdgeMargin + widest - width)
        return PopoverPlacement(
            frame: CGRect(x: x, y: barTop - popoverGap - height, width: width, height: height),
            pointerX: anchor.midX, maxHeight: maxHeight)
    }
}

/// One button of the mobile toolbar (docs/layout-reference §4.2). The phone
/// bar has no room for every Tool, so a family of Tools shares a slot: tapping
/// the slot's armed Tool again offers the others of its family.
struct MobileToolSlot: Equatable {
    /// The family, the slot's default Tool first.
    let tools: [Tool]

    /// Excalidraw's phone order; Insert image and Frame have no v1 Tool.
    static let all: [MobileToolSlot] = [
        [.hand], [.selection], [.freedraw], [.eraser],
        [.rectangle, .diamond, .ellipse], [.arrow, .line], [.text],
    ].map(MobileToolSlot.init)

    /// What the slot shows: the armed Tool if it is of the family, else the
    /// family's last chosen one, else its default.
    func shownTool(current: Tool, last: Tool?) -> Tool {
        if tools.contains(current) { return current }
        if let last, tools.contains(last) { return last }
        return tools[0]
    }
}
