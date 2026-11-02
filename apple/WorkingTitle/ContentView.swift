import SwiftUI

struct ContentView: View {
    /// An unreadable sample leaves an empty Canvas rather than a crash.
    @State private var geometry = ((try? SampleDrawing.makeEditor()) ?? Editor()).geometry()

    var body: some View {
        CanvasView(geometry: geometry)
            .ignoresSafeArea()
    }
}
