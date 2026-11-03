import SwiftUI

/// Minimal properties panel; its final placement (a left panel, or a sheet on
/// iPhone) comes with the layout. It sets the properties of the Selection, or
/// of the next Element if nothing is selected, and shows what the core says
/// they are; a value the Selection's Elements do not share shows no option
/// chosen. The option sets are Excalidraw's.
struct PropertiesPanel: View {
    let controller: CanvasController

    private static let strokeColors = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"]
    private static let backgroundColors = [
        "transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99",
    ]

    var body: some View {
        if let shown = controller.properties {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    section("Stroke") {
                        swatches(Self.strokeColors, selected: shown.strokeColor) {
                            .strokeColor($0)
                        }
                    }
                    section("Background") {
                        swatches(Self.backgroundColors, selected: shown.backgroundColor) {
                            .backgroundColor($0)
                        }
                    }
                    section("Fill") {
                        options(
                            [(.hachure, "Hachure"), (.crossHatch, "Cross-hatch"), (.solid, "Solid")],
                            selected: shown.fillStyle
                        ) { .fillStyle($0) }
                    }
                    section("Stroke width") {
                        options(
                            [(.thin, "Thin"), (.medium, "Bold"), (.bold, "Extra bold")],
                            selected: shown.strokeWidth
                        ) { .strokeWidth($0) }
                    }
                    section("Stroke style") {
                        options(
                            [(.solid, "Solid"), (.dashed, "Dashed"), (.dotted, "Dotted")],
                            selected: shown.strokeStyle
                        ) { .strokeStyle($0) }
                    }
                    section("Sloppiness") {
                        options(
                            [(.architect, "Architect"), (.artist, "Artist"), (.cartoonist, "Cartoonist")],
                            selected: shown.sloppiness
                        ) { .sloppiness($0) }
                    }
                    section("Edges") {
                        options([(.sharp, "Sharp"), (.round, "Round")], selected: shown.edges) {
                            .edges($0)
                        }
                    }
                    section("Opacity") {
                        Slider(
                            value: Binding(
                                get: { shown.opacity ?? 100 },
                                set: { controller.setProperty(.opacity($0)) }),
                            in: 0...100, step: 10)
                    }
                }
                .padding()
            }
            .frame(width: 220)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxHeight: 520)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            .padding(.leading)
            .padding(.top, 72)
        }
    }

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content)
        -> some View
    {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            content()
        }
    }

    private func swatches(
        _ colors: [String], selected: String?, property: @escaping (String) -> Property
    ) -> some View {
        HStack {
            ForEach(colors, id: \.self) { color in
                Button {
                    controller.setProperty(property(color))
                } label: {
                    Circle()
                        .fill(Color(hex: color))
                        .overlay(Circle().stroke(.secondary, lineWidth: 1))
                        .overlay(
                            Circle().stroke(Color.accentColor, lineWidth: 3)
                                .opacity(color == selected ? 1 : 0)
                        )
                        .frame(width: 28, height: 28)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(color)
            }
        }
    }

    private func options<Value: Hashable>(
        _ options: [(Value, String)], selected: Value?, property: @escaping (Value) -> Property
    ) -> some View {
        Picker(
            "",
            selection: Binding<Value?>(
                get: { selected },
                set: { if let value = $0 { controller.setProperty(property(value)) } })
        ) {
            ForEach(options, id: \.0) { value, name in Text(name).tag(Optional(value)) }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
    }
}

extension Color {
    /// A colour from `#rrggbb`; `transparent` and anything unreadable is clear.
    fileprivate init(hex: String) {
        guard hex.hasPrefix("#"), let value = UInt32(hex.dropFirst(), radix: 16) else {
            self = .clear
            return
        }
        self.init(
            red: Double((value >> 16) & 0xFF) / 255,
            green: Double((value >> 8) & 0xFF) / 255,
            blue: Double(value & 0xFF) / 255)
    }
}
