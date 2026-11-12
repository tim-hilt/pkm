import SwiftUI

// The groups of the Properties panel, each a view that can be placed on its
// own: given the controller and the panel state, it shows its option set
// under a title and sets the property through the controller. The caller
// decides whether a group applies (`shown.sections`, `controller.hasSelection`);
// the desktop panel stacks them, phone popovers pick the ones they need.

/// A titled group: a caption above its content.
struct PanelSection<Content: View>: View {
    let title: LocalizedStringResource
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            content
        }
    }
}

/// A row of icon buttons, one per option, the chosen one highlighted;
/// VoiceOver reads an option's name.
private struct OptionRow<Value: Hashable>: View {
    let options: [(Value, PanelIcon, LocalizedStringResource)]
    let selected: Value?
    let onPick: (Value) -> Void

    var body: some View {
        HStack(spacing: 8) {
            ForEach(options, id: \.0) { value, icon, name in
                IconButton(icon: icon, name: name, isSelected: selected == value) { onPick(value) }
            }
        }
    }
}

/// A row of icon buttons that act on the Selection.
private struct ActionRow: View {
    let buttons: [(PanelIcon, LocalizedStringResource, () -> Void)]

    var body: some View {
        HStack(spacing: 8) {
            ForEach(buttons.indices, id: \.self) { index in
                IconButton(icon: buttons[index].0, name: buttons[index].1, isSelected: false, action: buttons[index].2)
            }
        }
    }
}

struct StrokeGroup: View {
    static let topPicks = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"]
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Stroke") {
            ColorPickerView(kind: .stroke, topPicks: Self.topPicks, selected: shown.strokeColor) {
                controller.setProperty(.strokeColor($0))
            }
        }
    }
}

struct BackgroundGroup: View {
    static let topPicks = ["transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"]
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Background") {
            ColorPickerView(kind: .background, topPicks: Self.topPicks, selected: shown.backgroundColor) {
                controller.setProperty(.backgroundColor($0))
            }
        }
    }
}

struct FillGroup: View {
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Fill") {
            OptionRow(
                options: [
                    (.hachure, .drawing(PanelIcon.hachure), "Hachure"),
                    (.crossHatch, .drawing(PanelIcon.crossHatch), "Cross-hatch"),
                    (.solid, .drawing(PanelIcon.solidFill), "Solid"),
                ], selected: shown.fillStyle
            ) { controller.setProperty(.fillStyle($0)) }
        }
    }
}

struct StrokeWidthGroup: View {
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Stroke width") {
            OptionRow(
                options: [
                    (.thin, .drawing(PanelIcon.width(1)), "Thin"),
                    (.medium, .drawing(PanelIcon.width(2.5)), "Bold"),
                    (.bold, .drawing(PanelIcon.width(4.5)), "Extra bold"),
                ], selected: shown.strokeWidth
            ) { controller.setProperty(.strokeWidth($0)) }
        }
    }
}

struct StrokeStyleGroup: View {
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Stroke style") {
            OptionRow(
                options: [
                    (.solid, .drawing(PanelIcon.line(dash: [])), "Solid"),
                    (.dashed, .drawing(PanelIcon.line(dash: [4, 3])), "Dashed"),
                    (.dotted, .drawing(PanelIcon.line(dash: [0.1, 3], round: true)), "Dotted"),
                ], selected: shown.strokeStyle
            ) { controller.setProperty(.strokeStyle($0)) }
        }
    }
}

struct SloppinessGroup: View {
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Sloppiness") {
            OptionRow(
                options: [
                    (.architect, .drawing(PanelIcon.scribble(wobble: 0)), "Architect"),
                    (.artist, .drawing(PanelIcon.scribble(wobble: 1)), "Artist"),
                    (.cartoonist, .drawing(PanelIcon.scribble(wobble: 2)), "Cartoonist"),
                ], selected: shown.sloppiness
            ) { controller.setProperty(.sloppiness($0)) }
        }
    }
}

struct EdgesGroup: View {
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Edges") {
            OptionRow(
                options: [
                    (.sharp, .drawing(PanelIcon.corner(radius: 0)), "Sharp"),
                    (.round, .drawing(PanelIcon.corner(radius: 7)), "Round"),
                ], selected: shown.edges
            ) { controller.setProperty(.edges($0)) }
        }
    }
}

/// Font size and text alignment.
struct TextGroup: View {
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            PanelSection(title: "Font size") {
                OptionRow(
                    options: [
                        (.small, .text("S"), "Small"), (.medium, .text("M"), "Medium"),
                        (.large, .text("L"), "Large"), (.extraLarge, .text("XL"), "Extra large"),
                    ], selected: shown.fontSize
                ) { controller.setProperty(.fontSize($0)) }
            }
            PanelSection(title: "Text align") {
                OptionRow(
                    options: [
                        (.left, .symbol("text.alignleft"), "Left"),
                        (.center, .symbol("text.aligncenter"), "Center"),
                        (.right, .symbol("text.alignright"), "Right"),
                    ], selected: shown.textAlign
                ) { controller.setProperty(.textAlign($0)) }
            }
        }
    }
}

struct OpacityGroup: View {
    let controller: CanvasController
    let shown: Properties

    var body: some View {
        PanelSection(title: "Opacity") {
            Slider(
                value: Binding(
                    get: { shown.opacity ?? 100 },
                    set: { controller.setProperty(.opacity($0)) }),
                in: 0...100, step: 10
            )
            .accessibilityLabel(Text("Opacity"))
            HStack {
                Text("0")
                Spacer()
                Text("100")
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .accessibilityHidden(true)
        }
    }
}

struct LayersGroup: View {
    let controller: CanvasController

    var body: some View {
        PanelSection(title: "Layers") {
            ActionRow(buttons: [
                (.symbol("arrow.down.to.line"), "Send to Back", { controller.moveInZOrder(.toBack) }),
                (.symbol("arrow.down"), "Send Backward", { controller.moveInZOrder(.backward) }),
                (.symbol("arrow.up"), "Bring Forward", { controller.moveInZOrder(.forward) }),
                (.symbol("arrow.up.to.line"), "Bring to Front", { controller.moveInZOrder(.toFront) }),
            ])
        }
    }
}

/// Align and, for three or more groups or Elements, Distribute; shown where
/// the core says Align applies (`Sections.align`).
struct AlignGroup: View {
    let controller: CanvasController
    let sections: Sections

    var body: some View {
        PanelSection(title: "Align") {
            ActionRow(
                buttons: [
                    (.drawing(PanelIcon.arrange(.start)), "Align Left", { controller.align(.left) }),
                    (
                        .drawing(PanelIcon.arrange(.centre)), "Center Horizontally",
                        {
                            controller.align(.centerHorizontally)
                        }
                    ),
                    (.drawing(PanelIcon.arrange(.end)), "Align Right", { controller.align(.right) }),
                ]
                    + (sections.distribute
                        ? [
                            (
                                .drawing(PanelIcon.arrange(.spread)), "Distribute Horizontally",
                                {
                                    controller.distribute(.horizontally)
                                }
                            )
                        ] : []))
            ActionRow(
                buttons: [
                    (.drawing(PanelIcon.arrange(.start.transposed)), "Align Top", { controller.align(.top) }),
                    (
                        .drawing(PanelIcon.arrange(.centre.transposed)), "Center Vertically",
                        {
                            controller.align(.centerVertically)
                        }
                    ),
                    (.drawing(PanelIcon.arrange(.end.transposed)), "Align Bottom", { controller.align(.bottom) }),
                ]
                    + (sections.distribute
                        ? [
                            (
                                .drawing(PanelIcon.arrange(.spread.transposed)), "Distribute Vertically",
                                {
                                    controller.distribute(.vertically)
                                }
                            )
                        ] : []))
        }
    }
}

/// Duplicate and Delete; on the phone, whichever sit on the Action row instead are left out.
struct ActionsGroup: View {
    let controller: CanvasController
    var showsDuplicate = true
    var showsDelete = true

    var body: some View {
        if showsDuplicate || showsDelete {
            PanelSection(title: "Actions") {
                ActionRow(
                    buttons: (showsDuplicate
                        ? [(.symbol("plus.square.on.square"), "Duplicate", { controller.duplicate() })] : [])
                        + (showsDelete ? [(.symbol("trash"), "Delete", { controller.deleteSelection() })] : []))
            }
        }
    }
}
