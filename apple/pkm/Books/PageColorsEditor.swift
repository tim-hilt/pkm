import SwiftUI

/// Pickers for the Page color (paper) and Text color (ink) of Dark Pages, and a
/// Reset button: three rows in a Form, or children of the caller's stack in the Reader popover.
struct PageColorsEditor: View {
    @Environment(ReaderSettings.self) private var settings
    var resetTitle: LocalizedStringKey = "Reset"

    var body: some View {
        @Bindable var settings = settings
        Group {
            ColorPicker("Page", selection: binding($settings.pageColors.paper), supportsOpacity: false)
            ColorPicker("Text", selection: binding($settings.pageColors.ink), supportsOpacity: false)
            Button(resetTitle) { settings.resetPageColors() }
                .disabled(settings.pageColors == .default)
        }
    }

    private func binding(_ rgb: Binding<RGB>) -> Binding<Color> {
        Binding(get: { Color(rgb.wrappedValue) }, set: { rgb.wrappedValue = RGB($0) })
    }
}

extension Color {
    init(_ rgb: RGB) { self.init(.sRGB, red: rgb.r, green: rgb.g, blue: rgb.b) }
}

extension RGB {
    init(_ color: Color) {
        let resolved = color.resolve(in: EnvironmentValues())
        func clamp(_ x: Float) -> Double { Double(min(max(x, 0), 1)) }
        self.init(clamp(resolved.red), clamp(resolved.green), clamp(resolved.blue))
    }
}
