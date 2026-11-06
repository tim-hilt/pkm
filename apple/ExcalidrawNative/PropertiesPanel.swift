import SwiftUI

/// The properties panel: Excalidraw's 195 pt island, as tall as its content
/// and scrolling beyond the height its container offers; the layout places
/// it. It sets the properties of the Selection, or
/// of the next Element if nothing is selected, and shows what the core says
/// they are; a value the Selection's Elements do not share shows no option
/// chosen. The option sets are Excalidraw's.
struct PropertiesPanel: View {
    let controller: CanvasController
    /// The tallest the panel may be; beyond it, it scrolls.
    var maxHeight: CGFloat = .infinity
    /// Excalidraw's 195 pt; `nil` fills the width offered (the phone sheet).
    var width: CGFloat? = 195
    @State private var contentHeight: CGFloat = 0

    static let strokeColors = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"]
    static let backgroundColors = [
        "transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99",
    ]

    var body: some View {
        if let shown = controller.properties {
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    if shown.sections.strokeColor {
                        section("Stroke") {
                            ColorPickerView(
                                kind: .stroke, topPicks: Self.strokeColors, selected: shown.strokeColor
                            ) { controller.setProperty(.strokeColor($0)) }
                        }
                    }
                    if shown.sections.backgroundColor {
                        section("Background") {
                            ColorPickerView(
                                kind: .background, topPicks: Self.backgroundColors,
                                selected: shown.backgroundColor
                            ) { controller.setProperty(.backgroundColor($0)) }
                        }
                    }
                    if shown.sections.fillStyle {
                        section("Fill") {
                            options(
                                "Fill",
                                [(.hachure, "Hachure"), (.crossHatch, "Cross-hatch"), (.solid, "Solid")],
                                selected: shown.fillStyle
                            ) { .fillStyle($0) }
                        }
                    }
                    if shown.sections.strokeWidth {
                        section("Stroke width") {
                            options(
                                "Stroke width",
                                [(.thin, "Thin"), (.medium, "Bold"), (.bold, "Extra bold")],
                                selected: shown.strokeWidth
                            ) { .strokeWidth($0) }
                        }
                    }
                    if shown.sections.strokeStyle {
                        section("Stroke style") {
                            options(
                                "Stroke style",
                                [(.solid, "Solid"), (.dashed, "Dashed"), (.dotted, "Dotted")],
                                selected: shown.strokeStyle
                            ) { .strokeStyle($0) }
                        }
                    }
                    if shown.sections.sloppiness {
                        section("Sloppiness") {
                            options(
                                "Sloppiness",
                                [(.architect, "Architect"), (.artist, "Artist"), (.cartoonist, "Cartoonist")],
                                selected: shown.sloppiness
                            ) { .sloppiness($0) }
                        }
                    }
                    if shown.sections.edges {
                        section("Edges") {
                            options("Edges", [(.sharp, "Sharp"), (.round, "Round")], selected: shown.edges) {
                                .edges($0)
                            }
                        }
                    }
                    if shown.sections.text {
                        section("Font size") {
                            options(
                                "Font size",
                                [(.small, "S"), (.medium, "M"), (.large, "L"), (.extraLarge, "XL")],
                                spoken: [
                                    .small: "Small", .medium: "Medium", .large: "Large", .extraLarge: "Extra large",
                                ],
                                selected: shown.fontSize
                            ) { .fontSize($0) }
                        }
                        section("Text align") {
                            options(
                                "Text align",
                                [(.left, "Left"), (.center, "Center"), (.right, "Right")],
                                selected: shown.textAlign
                            ) { .textAlign($0) }
                        }
                    }
                    if shown.sections.opacity {
                        section("Opacity") {
                            Slider(
                                value: Binding(
                                    get: { shown.opacity ?? 100 },
                                    set: { controller.setProperty(.opacity($0)) }),
                                in: 0...100, step: 10
                            )
                            .accessibilityLabel(Text("Opacity"))
                        }
                    }
                }
                .padding(12)
                .onGeometryChange(for: CGFloat.self, of: \.size.height) { contentHeight = $0 }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(width: width, height: min(contentHeight, maxHeight))
            .frame(maxWidth: width == nil ? .infinity : nil)
            .background(.background, in: RoundedRectangle(cornerRadius: 8))
            .shadow(color: .black.opacity(0.17), radius: 0.5)
            .shadow(color: .black.opacity(0.05), radius: 7, y: 7)
        }
    }

    private func section<Content: View>(_ title: LocalizedStringResource, @ViewBuilder content: () -> Content)
        -> some View
    {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            content()
        }
    }

    /// A segmented control of `options`; VoiceOver reads an option's
    /// `spoken` name where its shown name is an abbreviation.
    private func options<Value: Hashable>(
        _ title: LocalizedStringResource, _ options: [(Value, LocalizedStringResource)],
        spoken: [Value: LocalizedStringResource] = [:], selected: Value?,
        property: @escaping (Value) -> Property
    ) -> some View {
        Picker(
            String(localized: title),
            selection: Binding<Value?>(
                get: { selected },
                set: { if let value = $0 { controller.setProperty(property(value)) } })
        ) {
            ForEach(options, id: \.0) { value, name in
                Text(name).accessibilityLabel(Text(spoken[value] ?? name)).tag(Optional(value))
            }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
    }
}
