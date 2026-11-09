import CoreGraphics

/// One control of the phone Action row.
enum ActionRowItem: Hashable {
    case stroke, background, properties, text, actions
    /// On the row itself only when it is wide enough, else inside `actions`.
    case duplicate, delete

    /// Whether the item opens a popover (the others act at once).
    var opensPopover: Bool { self != .delete && self != .duplicate }
}

/// The phone Action row (docs/layout-reference §4.3) as a pure value: which
/// items it shows and which popover is open. The view renders it and forwards
/// events; it holds no logic of its own.
struct ActionRowModel: Equatable {
    /// Row width from which Delete, then Duplicate, sit on the row (excalidraw.com's).
    static let deleteWidth: CGFloat = 374
    static let duplicateWidth: CGFloat = 412

    private(set) var items: [ActionRowItem] = []
    /// The open popover's trigger.
    private(set) var open: ActionRowItem?

    /// The items for what the core says applies (`nil` sections: nothing to
    /// style) and whether there is a Selection, for a row `width` wide.
    static func items(sections: Sections?, hasSelection: Bool, width: CGFloat) -> [ActionRowItem] {
        var items: [ActionRowItem] = []
        if let s = sections {
            if s.strokeColor { items.append(.stroke) }
            if s.backgroundColor { items.append(.background) }
            if s.fillStyle || s.strokeWidth || s.strokeStyle || s.sloppiness || s.edges || s.opacity {
                items.append(.properties)
            }
            if s.text { items.append(.text) }
        }
        if hasSelection {
            items.append(.actions)
            // excalidraw.com's order, Duplicate before Delete; Delete is the first to fit.
            if width >= duplicateWidth { items.append(.duplicate) }
            if width >= deleteWidth { items.append(.delete) }
        }
        return items
    }

    /// Whether the `…` popover still offers Duplicate / Delete: only while they are not on the row.
    var popoverHasDuplicate: Bool { !items.contains(.duplicate) }
    var popoverHasDelete: Bool { !items.contains(.delete) }

    /// The row's inputs changed: a popover whose trigger is gone closes.
    mutating func update(sections: Sections?, hasSelection: Bool, width: CGFloat) {
        items = Self.items(sections: sections, hasSelection: hasSelection, width: width)
        if let open, !items.contains(open) { self.open = nil }
    }

    /// A tap on a popover trigger: opens it, switches to it, or closes it if it is the open one.
    mutating func tap(_ item: ActionRowItem) {
        guard item.opensPopover, items.contains(item) else { return }
        open = open == item ? nil : item
    }

    /// Closes the open popover.
    mutating func close() { open = nil }

    /// A touch on the Canvas: closes the open popover; true if it did, so
    /// that the touch must not reach the Canvas (ADR-0013).
    mutating func canvasTouch() -> Bool {
        defer { open = nil }
        return open != nil
    }
}
