import SwiftUI

/// The phone action row over the toolbar (docs/layout-reference §4.3): one
/// trigger per `ActionRowModel` item. It renders the model and forwards taps;
/// the popovers themselves are placed by `DrawingView` (`ActionPopoverContent`).
/// Undo and redo have no buttons: they are the system gestures.
struct MobileActionRow: View {
    let controller: CanvasController
    let model: ActionRowModel
    let onTap: (ActionRowItem) -> Void
    /// Where each popover trigger is, in the chrome's coordinate space.
    @Binding var anchors: [ActionRowItem: CGRect]
    let space: String

    var body: some View {
        HStack(spacing: 6) {
            ForEach(model.items, id: \.self) { item in
                trigger(item)
                    .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .named(space)) }) { anchors[item] = $0 }
            }
            Spacer(minLength: 0)
        }
        .frame(height: 43.2)
    }

    @ViewBuilder private func trigger(_ item: ActionRowItem) -> some View {
        let open = model.open == item
        switch item {
        case .stroke:
            SwatchTrigger(
                name: "Stroke", color: controller.properties?.strokeColor, isOpen: open, hatched: true
            ) { onTap(item) }
        case .background:
            SwatchTrigger(
                name: "Background", color: controller.properties?.backgroundColor, isOpen: open, hatched: false
            ) { onTap(item) }
        case .properties: IconTrigger(name: "Properties", symbol: "slider.horizontal.3", isOpen: open) { onTap(item) }
        case .text: IconTrigger(name: "Text", symbol: "textformat.size", isOpen: open) { onTap(item) }
        case .actions: IconTrigger(name: "Actions", symbol: "ellipsis", isOpen: open) { onTap(item) }
        case .delete: IconTrigger(name: "Delete", symbol: "trash", isOpen: false) { controller.deleteSelection() }
        case .duplicate:
            IconTrigger(name: "Duplicate", symbol: "plus.square.on.square", isOpen: false) { controller.duplicate() }
        }
    }
}

/// A 32 pt icon trigger, radius 8, its hit area grown to 44 pt; the checked
/// fill while its popover is open.
private struct IconTrigger: View {
    let name: LocalizedStringResource
    let symbol: String
    let isOpen: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 15))
                .foregroundStyle(isOpen ? Chrome.checkedGlyph : .primary)
                .frame(width: 32, height: 32)
                .background(isOpen ? Chrome.checkedFill : Chrome.island, in: RoundedRectangle(cornerRadius: 8))
                .contentShape(Rectangle().inset(by: -6))
        }
        .buttonStyle(.plain)
        .buttonPointer()
        .accessibilityLabel(Text(name))
        .accessibilityAddTraits(isOpen ? .isSelected : [])
    }
}

/// A 32 pt colour swatch trigger, radius 5, with a divider-coloured border:
/// the current colour (a checkerboard when transparent; the stroke one carries
/// Excalidraw's hatch glyph). VoiceOver reads the colour's name.
private struct SwatchTrigger: View {
    let name: LocalizedStringResource
    /// `nil` when the Selection's Elements do not share a colour.
    let color: String?
    let isOpen: Bool
    let hatched: Bool
    let action: () -> Void
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let shown = ColorPalette.displayColor(color ?? "transparent", dark: colorScheme == .dark)
        let shape = RoundedRectangle(cornerRadius: 5)
        Button(action: action) {
            ZStack {
                if isOpen { shape.fill(Chrome.checkedFill) }
                ColorSwatch(color: shown, size: 26)
                    .overlay {
                        if hatched {
                            PanelIcon.drawing(PanelIcon.hachure).view
                                .foregroundStyle(Color(hex: shown).isLight ? .black.opacity(0.7) : .white.opacity(0.85))
                        }
                    }
            }
            .frame(width: 32, height: 32)
            .overlay(shape.strokeBorder(Chrome.divider, lineWidth: 1))
            .contentShape(Rectangle().inset(by: -6))
        }
        .buttonStyle(.plain)
        .buttonPointer()
        .accessibilityLabel(Text(name))
        .accessibilityValue(color.map { Text(ColorPalette.name($0)) } ?? Text(""))
        .accessibilityAddTraits(isOpen ? .isSelected : [])
    }
}

extension Color {
    /// Whether black reads better than white on the colour; clear counts as light.
    fileprivate var isLight: Bool {
        let resolved = resolve(in: EnvironmentValues())
        guard resolved.opacity > 0.5 else { return true }
        return 0.299 * resolved.red + 0.587 * resolved.green + 0.114 * resolved.blue > 0.6
    }
}

/// What an open Action row popover shows: the groups of its trigger that
/// apply, built from the same pieces as the desktop panel.
struct ActionPopoverContent: View {
    let item: ActionRowItem
    let model: ActionRowModel
    let controller: CanvasController

    var body: some View {
        // A Selection without style groups (no `properties`) still has Layers and Actions.
        if item == .actions {
            VStack(alignment: .leading, spacing: 8) {
                LayersGroup(controller: controller)
                if let sections = controller.properties?.sections, sections.align {
                    AlignGroup(controller: controller, sections: sections)
                }
                ActionsGroup(
                    controller: controller, showsDuplicate: model.popoverHasDuplicate,
                    showsDelete: model.popoverHasDelete)
            }
        } else if let shown = controller.properties {
            switch item {
            case .stroke:
                ColorPopover(title: "Stroke", kind: .stroke, selected: shown.strokeColor) {
                    controller.setProperty(.strokeColor($0))
                }
            case .background:
                ColorPopover(title: "Background", kind: .background, selected: shown.backgroundColor) {
                    controller.setProperty(.backgroundColor($0))
                }
            case .properties:
                VStack(alignment: .leading, spacing: 8) {
                    if shown.sections.fillStyle { FillGroup(controller: controller, shown: shown) }
                    if shown.sections.strokeWidth { StrokeWidthGroup(controller: controller, shown: shown) }
                    if shown.sections.strokeStyle { StrokeStyleGroup(controller: controller, shown: shown) }
                    if shown.sections.sloppiness { SloppinessGroup(controller: controller, shown: shown) }
                    if shown.sections.edges { EdgesGroup(controller: controller, shown: shown) }
                    if shown.sections.opacity { OpacityGroup(controller: controller, shown: shown) }
                }
                .frame(width: 176, alignment: .leading)
            case .text:
                TextGroup(controller: controller, shown: shown)
            case .actions, .delete, .duplicate: EmptyView()
            }
        }
    }
}

/// A colour popover: a title, then the full picker at once, without top picks.
private struct ColorPopover: View {
    let title: LocalizedStringResource
    let kind: ColorPalette.Kind
    let selected: String?
    let onPick: (String) -> Void

    var body: some View {
        PanelSection(title: title) {
            FullColorPicker(kind: kind, selected: selected, onPick: onPick)
                .frame(width: 196)
        }
    }
}
