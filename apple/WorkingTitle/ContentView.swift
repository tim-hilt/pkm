import SwiftUI

struct ContentView: View {
    @State private var geometry = Editor().geometry()

    var body: some View {
        CanvasView(geometry: geometry)
            .ignoresSafeArea()
    }
}
