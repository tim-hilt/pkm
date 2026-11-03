import SwiftUI

/// The floating Action Bar next to the Selection, in place of a context menu.
/// The core says whether it is shown and where; this only draws it there and
/// keeps it inside the Canvas. Duplicate and delete work; the "more" entry
/// point is where z-order and grouping will go.
struct ActionBarView: View {
    let controller: CanvasController

    /// Apple's touch target; the bar's size follows from it.
    private static let button: CGFloat = 44
    private static let buttons = 3
    private static let padding: CGFloat = 4
    private static let size = CGSize(
        width: button * CGFloat(buttons) + padding * 2, height: button + padding * 2)

    var body: some View {
        // Clear and flexible, so that the bar can be placed in the Canvas's
        // coordinates (which reach under the safe areas) without catching
        // the touches meant for the Canvas.
        Color.clear
            .ignoresSafeArea()
            .overlay(alignment: .topLeading) {
                if let anchor = controller.actionBar {
                    bar.offset(origin(for: anchor))
                }
            }
    }

    private var bar: some View {
        HStack(spacing: 0) {
            Button("Duplicate", systemImage: "plus.square.on.square") { controller.duplicate() }
            Button("Delete", systemImage: "trash", role: .destructive) { controller.deleteSelection() }
            // Z-order and group come with their own tickets.
            Button("More", systemImage: "ellipsis") {}
                .disabled(true)
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .frame(width: Self.size.width, height: Self.size.height)
        .background(.regularMaterial, in: Capsule())
        .shadow(radius: 4, y: 1)
    }

    /// The top-left corner of the bar whose top edge is centred on `anchor`,
    /// moved as far as needed to keep it in the Canvas.
    private func origin(for anchor: ActionBar) -> CGSize {
        let view = controller.viewSize
        let x = anchor.x - Self.size.width / 2
        let maxX = max(0, view.width - Self.size.width)
        let maxY = max(0, view.height - Self.size.height)
        return CGSize(width: min(max(x, 0), maxX), height: min(max(anchor.y, 0), maxY))
    }
}
