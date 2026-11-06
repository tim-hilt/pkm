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
    /// scrolls beyond this, clear of the zoom controls.
    /// The panel's room shrinks by however much higher than `propertiesTop` it sits.
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
