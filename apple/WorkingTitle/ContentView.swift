import SwiftUI

struct ContentView: View {
    @State private var editor = Editor()

    var body: some View {
        VStack(spacing: 8) {
            Text("WorkingTitle")
            Text("Elements: \(editor.elementCount())")
                .foregroundStyle(.secondary)
        }
    }
}
