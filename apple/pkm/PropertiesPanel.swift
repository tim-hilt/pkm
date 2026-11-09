import SwiftUI

/// The properties panel: Excalidraw's 195 pt island, as tall as its content
/// and scrolling beyond the height its container offers; the layout places
/// it. It sets the properties of the Selection, or
/// of the next Element if nothing is selected, and shows what the core says
/// they are; a value the Selection's Elements do not share shows no option
/// chosen. The option sets are Excalidraw's, shown as icons like Excalidraw's
/// (`PanelIcon`; VoiceOver reads their names). With a Selection the panel also
/// has its layer (z-order) and action (duplicate, delete) buttons.
struct PropertiesPanel: View {
    let controller: CanvasController
    /// The tallest the panel may be; beyond it, it scrolls.
    var maxHeight: CGFloat = .infinity
    /// Excalidraw's 195 pt.
    private let width: CGFloat = 195
    @State private var contentHeight: CGFloat = 0

    static let strokeColors = StrokeGroup.topPicks
    static let backgroundColors = BackgroundGroup.topPicks

    var body: some View {
        if let shown = controller.properties {
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    if shown.sections.strokeColor { StrokeGroup(controller: controller, shown: shown) }
                    if shown.sections.backgroundColor { BackgroundGroup(controller: controller, shown: shown) }
                    if shown.sections.fillStyle { FillGroup(controller: controller, shown: shown) }
                    if shown.sections.strokeWidth { StrokeWidthGroup(controller: controller, shown: shown) }
                    if shown.sections.strokeStyle { StrokeStyleGroup(controller: controller, shown: shown) }
                    if shown.sections.sloppiness { SloppinessGroup(controller: controller, shown: shown) }
                    if shown.sections.edges { EdgesGroup(controller: controller, shown: shown) }
                    if shown.sections.text { TextGroup(controller: controller, shown: shown) }
                    if shown.sections.opacity { OpacityGroup(controller: controller, shown: shown) }
                    if controller.hasSelection {
                        LayersGroup(controller: controller)
                        ActionsGroup(controller: controller)
                    }
                }
                .padding(12)
                .onGeometryChange(for: CGFloat.self, of: \.size.height) { contentHeight = $0 }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(width: width, height: min(contentHeight, maxHeight))
            .background(Chrome.island, in: RoundedRectangle(cornerRadius: 8))
            .shadow(color: .black.opacity(0.17), radius: 0.5)
            .shadow(color: .black.opacity(0.05), radius: 7, y: 7)
        }
    }
}

/// One 32 pt icon button of the panel (hit area grown to Apple's 44 pt).
struct IconButton: View {
    let icon: PanelIcon
    let name: LocalizedStringResource
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            icon.view
                .frame(width: 32, height: 32)
                .background(
                    isSelected ? Chrome.optionSelected : Color.primary.opacity(0.05),
                    in: RoundedRectangle(cornerRadius: 8)
                )
                .contentShape(Rectangle().inset(by: -6))
        }
        .buttonStyle(.plain)
        .buttonPointer()
        .accessibilityLabel(Text(name))
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

/// The glyph of a panel option: a system symbol, a short text, or a drawing
/// in a 20 pt square made with the foreground colour.
enum PanelIcon {
    case symbol(String)
    case text(String)
    case drawing(@Sendable (inout GraphicsContext, CGSize) -> Void)

    @ViewBuilder var view: some View {
        switch self {
        case .symbol(let name): Image(systemName: name).font(.system(size: 15))
        case .text(let text): Text(text).font(.system(size: 13, weight: .medium))
        case .drawing(let draw):
            Canvas { context, size in draw(&context, size) }.frame(width: 20, height: 20)
        }
    }

    private static let box = CGRect(x: 2, y: 2, width: 16, height: 16)
    private static func rounded(_ rect: CGRect) -> Path { Path(roundedRect: rect, cornerRadius: 3) }

    /// Diagonal lines (and, mirrored, a second set) clipped to the square.
    private static func hatching(_ context: inout GraphicsContext, crossed: Bool) {
        var inner = context
        inner.clip(to: rounded(box))
        var lines = Path()
        for offset in stride(from: -16.0, through: 16, by: 4.5) {
            lines.move(to: CGPoint(x: box.minX + offset, y: box.maxY))
            lines.addLine(to: CGPoint(x: box.minX + offset + 16, y: box.minY))
            if crossed {
                lines.move(to: CGPoint(x: box.minX + offset, y: box.minY))
                lines.addLine(to: CGPoint(x: box.minX + offset + 16, y: box.maxY))
            }
        }
        inner.stroke(lines, with: .foreground, lineWidth: 1.2)
        context.stroke(rounded(box), with: .foreground, lineWidth: 1.2)
    }

    static let hachure: @Sendable (inout GraphicsContext, CGSize) -> Void = { context, _ in
        hatching(&context, crossed: false)
    }
    static let crossHatch: @Sendable (inout GraphicsContext, CGSize) -> Void = { context, _ in
        hatching(&context, crossed: true)
    }
    static let solidFill: @Sendable (inout GraphicsContext, CGSize) -> Void = { context, _ in
        context.fill(rounded(box), with: .foreground)
    }

    /// A horizontal line of `width` pt.
    static func width(_ width: CGFloat) -> @Sendable (inout GraphicsContext, CGSize) -> Void {
        { context, _ in
            var line = Path()
            line.move(to: CGPoint(x: 3, y: 10))
            line.addLine(to: CGPoint(x: 17, y: 10))
            context.stroke(line, with: .foreground, style: StrokeStyle(lineWidth: width, lineCap: .butt))
        }
    }

    /// A horizontal line with the given dash pattern.
    static func line(dash: [CGFloat], round: Bool = false) -> @Sendable (inout GraphicsContext, CGSize) -> Void {
        { context, _ in
            var line = Path()
            line.move(to: CGPoint(x: 2, y: 10))
            line.addLine(to: CGPoint(x: 18, y: 10))
            context.stroke(
                line, with: .foreground,
                style: StrokeStyle(lineWidth: round ? 2 : 1.6, lineCap: round ? .round : .butt, dash: dash))
        }
    }

    /// A stroke that is the straighter, the smaller `wobble` (0 to 2) is.
    static func scribble(wobble: Int) -> @Sendable (inout GraphicsContext, CGSize) -> Void {
        { context, _ in
            let amplitude = CGFloat(wobble) * 2 + 1
            var path = Path()
            path.move(to: CGPoint(x: 2, y: 13))
            path.addCurve(
                to: CGPoint(x: 18, y: 7), control1: CGPoint(x: 7, y: 13 - amplitude * 2),
                control2: CGPoint(x: 12, y: 7 + amplitude * 2))
            let style = StrokeStyle(lineWidth: 1.2 + CGFloat(wobble) * 0.5, lineCap: .round)
            context.stroke(path, with: .foreground, style: style)
            if wobble == 2 {
                context.stroke(
                    path.offsetBy(dx: 0, dy: 2.5), with: .foreground,
                    style: StrokeStyle(lineWidth: 1, lineCap: .round))
            }
        }
    }

    /// A square whose corners have `radius`.
    static func corner(radius: CGFloat) -> @Sendable (inout GraphicsContext, CGSize) -> Void {
        { context, _ in
            context.stroke(Path(roundedRect: box, cornerRadius: radius / 2), with: .foreground, lineWidth: 1.6)
        }
    }
}
