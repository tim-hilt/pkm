import CoreGraphics

/// Where over a row of a Group a dragged Task is: its top quarter drops before the row, its middle
/// half onto it, its bottom quarter after it.
enum DropZone: Equatable {
    case before, onto, after

    /// The zone at `y` from the top of a row `height` high.
    init(y: CGFloat, height: CGFloat) {
        let at = height > 0 ? y / height : 0.5
        self = at < 0.25 ? .before : at > 0.75 ? .after : .onto
    }
}

/// What a drop of a Task on a row of its Group does.
enum RowDrop: Equatable {
    /// Moves it before the row `before`; `nil`: to the end of the Group.
    case move(before: String?)
    /// Makes it a Subtask of the row `parent`.
    case onto(parent: String)

    /// The drop of `dragged` at `zone` of `row` in a Group of the rows `rows` (by ID, in order);
    /// `row` nil is the space below the rows. None on the dragged row itself or for a Task or row
    /// outside the Group.
    init?(dragging dragged: String, on row: String?, at zone: DropZone, rows: [String]) {
        guard dragged != row, rows.contains(dragged) else { return nil }
        guard let row else {
            self = .move(before: nil)
            return
        }
        guard let index = rows.firstIndex(of: row) else { return nil }
        switch zone {
        case .before: self = .move(before: row)
        case .onto: self = .onto(parent: row)
        case .after: self = .move(before: rows.indices.contains(index + 1) ? rows[index + 1] : nil)
        }
    }
}
