import SwiftUI

/// The phone's own popover (ADR-0013): excalidraw.com's island with a pointer
/// tip toward its anchor. It sits per `DrawingLayout.popoverPlacement`, scrolls
/// beyond its maximum height, and for VoiceOver is a modal that its escape
/// gesture closes.
///
/// Place it in a container-sized view whose coordinate space is the one the
/// anchor and bar frames are measured in. Fading in and out is up to whoever
/// inserts and removes it (`.transition(.opacity)` is built in).
struct PhonePopover<Content: View>: View {
    let anchor: CGRect
    let container: CGSize
    let barTop: CGFloat
    let onClose: () -> Void
    @ViewBuilder let content: Content
    @State private var contentSize: CGSize = .zero
    @AccessibilityFocusState private var focused: Bool

    private static var padding: CGFloat { 12 }
    private static var tip: CGSize { CGSize(width: 14, height: 7) }

    var body: some View {
        let outer = CGSize(
            width: contentSize.width + 2 * Self.padding, height: contentSize.height + 2 * Self.padding)
        let placement = DrawingLayout.popoverPlacement(
            anchor: anchor, content: outer, container: container, barTop: barTop)
        let frame = placement.frame
        ScrollView {
            content
                .fixedSize()
                .padding(Self.padding)
                .onGeometryChange(for: CGSize.self, of: { $0.size }) { contentSize = $0 - 2 * Self.padding }
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(width: frame.width, height: frame.height)
        .background(Chrome.island, in: RoundedRectangle(cornerRadius: 8))
        .shadow(color: .black.opacity(0.17), radius: 0.5)
        .shadow(color: .black.opacity(0.08), radius: 1.5)
        .shadow(color: .black.opacity(0.05), radius: 7, y: 7)
        .overlay(alignment: .bottomLeading) {
            PointerTip()
                .fill(Chrome.island)
                .frame(width: Self.tip.width, height: Self.tip.height)
                .offset(x: placement.pointerX - frame.minX - Self.tip.width / 2, y: Self.tip.height)
                .accessibilityHidden(true)
        }
        .opacity(contentSize == .zero ? 0 : 1)
        .accessibilityElement(children: .contain)
        .accessibilityAddTraits(.isModal)
        .accessibilityAction(.escape, onClose)
        .accessibilityFocused($focused)
        .onAppear { focused = true }
        .position(x: frame.midX, y: frame.midY)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .transition(.opacity)
    }
}

extension CGSize {
    fileprivate static func - (size: CGSize, inset: CGFloat) -> CGSize {
        CGSize(width: size.width - inset, height: size.height - inset)
    }
}

/// A downward-pointing triangle.
private struct PointerTip: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

/// Between the Canvas and the chrome while a popover is open: takes every
/// touch (tap, drag, pinch) so that it only closes the popover and never
/// reaches the Canvas. It outlives the popover by the touch that closed it, so
/// that the rest of that gesture, a pinch's second finger, is not handed to the
/// Canvas either.
struct PopoverScrim: ViewModifier {
    let isOpen: Bool
    let onClose: () -> Void
    @GestureState private var touching = false

    func body(content: Content) -> some View {
        ZStack {
            if isOpen || touching {
                Color.clear
                    .contentShape(Rectangle())
                    .ignoresSafeArea()
                    .gesture(
                        DragGesture(minimumDistance: 0, coordinateSpace: .global)
                            .updating($touching) { _, state, _ in state = true }
                            .onChanged { _ in if isOpen { onClose() } }
                    )
                    .accessibilityHidden(true)
            }
            content
        }
    }
}

extension View {
    /// See `PopoverScrim`.
    func popoverScrim(isOpen: Bool, onClose: @escaping () -> Void) -> some View {
        modifier(PopoverScrim(isOpen: isOpen, onClose: onClose))
    }
}
