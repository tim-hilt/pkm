import SwiftUI

/// A colour row of the properties panel: the five top picks and, at the end,
/// a button showing the current colour that opens the full picker
/// (`FullColorPicker`) below the row. Everything is a tap target of the row's
/// width, so the full palette is reachable with a finger.
struct ColorPickerView: View {
    let kind: ColorPalette.Kind
    let topPicks: [String]
    /// The colour of the Selection; `nil` if its Elements do not share one.
    let selected: String?
    let onPick: (String) -> Void
    @Environment(\.colorScheme) private var colorScheme
    @State private var isOpen = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 0) {
                ForEach(topPicks, id: \.self) {
                    SwatchButton(color: $0, size: 22, selected: selected, onPick: onPick)
                }
                Divider().frame(height: 22).padding(.horizontal, 4)
                Button {
                    withAnimation(.snappy(duration: 0.2)) { isOpen.toggle() }
                } label: {
                    ColorSwatch(
                        color: ColorPalette.displayColor(selected ?? "transparent", dark: colorScheme == .dark),
                        size: 26
                    )
                    .overlay(Selection(size: 26).opacity(isOpen ? 1 : 0))
                    .frame(width: 26, height: 26)
                    .frame(minWidth: 30, minHeight: 36)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .buttonPointer()
                .accessibilityLabel(Text("More colours"))
                .accessibilityValue(selected.map { Text(ColorPalette.name($0)) } ?? Text(""))
                .accessibilityAddTraits(isOpen ? .isSelected : [])
            }
            if isOpen {
                FullColorPicker(kind: kind, selected: selected, onPick: onPick) {
                    withAnimation(.snappy(duration: 0.2)) { isOpen = false }
                }
                .padding(10)
                .background(Chrome.pickerPanel, in: RoundedRectangle(cornerRadius: 8))
            }
        }
    }
}

/// The full colour picker, without the top-picks row: Excalidraw's palette
/// grid, the shades of the current colour and a hex code field. It has no
/// background or padding of its own; its container provides them.
struct FullColorPicker: View {
    let kind: ColorPalette.Kind
    /// The colour of the Selection; `nil` if its Elements do not share one.
    let selected: String?
    let onPick: (String) -> Void
    /// Called after a swatch (not the hex field) was picked.
    var onSwatchPick: () -> Void = {}
    @State private var hex = ""
    @State private var hexIsInvalid = false
    @FocusState private var hexIsFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            heading("Colors")
            let grid = ColorPalette.grid(for: kind)
            ForEach(0..<3, id: \.self) { row in
                HStack(spacing: 0) {
                    ForEach(grid[row * 5..<row * 5 + 5], id: \.self) {
                        SwatchButton(color: $0, size: 26, selected: selected, onPick: swatchPick)
                    }
                }
            }
            heading("Shades")
            if let shades = ColorPalette.shades(of: selected) {
                HStack(spacing: 0) {
                    ForEach(shades, id: \.self) {
                        SwatchButton(color: $0, size: 26, selected: selected, onPick: swatchPick)
                    }
                }
            } else {
                Text("No shades available for this color").font(.caption).foregroundStyle(.secondary)
            }
            heading("Hex code")
            hexField
        }
        .onChange(of: selected, initial: true) { _, color in
            guard !hexIsFocused else { return }
            hex = Self.hexDigits(of: color)
            hexIsInvalid = false
        }
    }

    private func swatchPick(_ color: String) {
        onPick(color)
        onSwatchPick()
    }

    private var hexField: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 2) {
                Text("#").foregroundStyle(.secondary)
                TextField("Hex code", text: $hex)
                    .focused($hexIsFocused)
                    .autocorrectionDisabled()
                    #if os(iOS)
                        .textInputAutocapitalization(.never)
                        .keyboardType(.asciiCapable)
                    #endif
                    .submitLabel(.done)
                    .onSubmit(commitHex)
                    .onChange(of: hexIsFocused) { _, focused in if !focused { commitHex() } }
            }
            .padding(.horizontal, 8)
            .frame(minHeight: 36)
            .background(Chrome.inputFill, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Chrome.inputBorder, lineWidth: 1))
            if hexIsInvalid {
                Text("Not a valid color").font(.caption).foregroundStyle(.red)
            }
        }
    }

    private func commitHex() {
        if hex == Self.hexDigits(of: selected) {
            hexIsInvalid = false
        } else if let color = ColorPalette.color(fromHex: hex) {
            hexIsInvalid = false
            onPick(color)
        } else {
            hexIsInvalid = !hex.isEmpty
        }
    }

    private func heading(_ title: LocalizedStringResource) -> some View {
        Text(title).font(.caption).foregroundStyle(.secondary)
    }

    /// The digits a hex field shows for `color`: none for transparent.
    private static func hexDigits(of color: String?) -> String {
        guard let color, color.hasPrefix("#") else { return "" }
        return String(color.dropFirst())
    }
}

/// A swatch in a cell that shares the row's width, so that it is a
/// touch target well beyond its drawn size.
private struct SwatchButton: View {
    let color: String
    let size: CGFloat
    let selected: String?
    let onPick: (String) -> Void
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Button {
            onPick(color)
        } label: {
            ColorSwatch(color: ColorPalette.displayColor(color, dark: colorScheme == .dark), size: size)
                .overlay(Selection(size: size).opacity(color == selected ? 1 : 0))
                .frame(width: size, height: size)
                .frame(maxWidth: .infinity, minHeight: 36)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .buttonPointer()
        .accessibilityLabel(Text(ColorPalette.name(color)))
        .accessibilityAddTraits(color == selected ? .isSelected : [])
    }
}

/// A colour filled into a rounded square of `size`, with a hairline ring so
/// that white shows; transparent is a checkerboard, as in Excalidraw.
struct ColorSwatch: View {
    let color: String
    let size: CGFloat

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.2)
        shape.fill(Color(hex: color))
            .background {
                if Color.isTransparent(color) { Checkerboard().clipShape(shape) }
            }
            .overlay(shape.strokeBorder(Color.black.opacity(0.1), lineWidth: 1))
    }
}

/// The ring around the chosen swatch, drawn just outside it; the gaps
/// between swatches are wider than it.
private struct Selection: View {
    let size: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: size * 0.2 + 2)
            .strokeBorder(Color.accentColor, lineWidth: 2)
            .padding(-2)
    }
}

private struct Checkerboard: View {
    var body: some View {
        Canvas { context, size in
            let cell = size.width / 3
            context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(Chrome.checkerboard))
            for row in 0..<3 {
                for column in 0..<3 where (row + column) % 2 == 0 {
                    context.fill(
                        Path(CGRect(x: CGFloat(column) * cell, y: CGFloat(row) * cell, width: cell, height: cell)),
                        with: .color(.gray.opacity(0.35)))
                }
            }
        }
    }
}

extension Color {
    /// A colour from `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`; `transparent`
    /// and anything unreadable is clear.
    init(hex: String) {
        var digits = hex.hasPrefix("#") ? String(hex.dropFirst()) : ""
        if digits.count == 3 || digits.count == 4 { digits = digits.flatMap { [$0, $0] }.map(String.init).joined() }
        guard digits.count == 6 || digits.count == 8, let value = UInt32(digits, radix: 16) else {
            self = .clear
            return
        }
        let rgba = digits.count == 6 ? value << 8 | 0xFF : value
        self.init(
            red: Double(rgba >> 24 & 0xFF) / 255, green: Double(rgba >> 16 & 0xFF) / 255,
            blue: Double(rgba >> 8 & 0xFF) / 255, opacity: Double(rgba & 0xFF) / 255)
    }

    /// Whether `color` draws nothing: `transparent` or a zero alpha.
    static func isTransparent(_ color: String) -> Bool {
        guard color.hasPrefix("#") else { return true }
        let digits = color.dropFirst()
        switch digits.count {
        case 4: return digits.last == "0"
        case 8: return digits.suffix(2) == "00"
        default: return false
        }
    }
}
