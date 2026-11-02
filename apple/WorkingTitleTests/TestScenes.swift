import Foundation

@testable import WorkingTitle

/// Scenes for renderer tests, made the way the app makes its own: by loading
/// a Drawing into the core's `Editor`.
enum TestScenes {
    /// A Drawing with one rectangle Element at (50, 100), 280 x 180, drawn
    /// with exact edges (roughness 0), so renderer tests can probe them.
    static func rectangle(extra: String = "") -> Geometry {
        geometry(of: drawing(rectangle: extra))
    }

    static func drawing(rectangle extra: String = "") -> String {
        """
        {"type":"excalidraw","version":2,"elements":[
          {"id":"box","type":"rectangle","x":50,"y":100,"width":280,"height":180,"roughness":0\(extra)}]}
        """
    }

    static func geometry(of drawing: String) -> Geometry {
        let editor = Editor()
        try! editor.load(bytes: Data(drawing.utf8))
        return editor.geometry()
    }

    /// Shapes in the styles hand-drawn shapes come in: dashed, dotted,
    /// translucent and thick, rounded, every sloppiness. Seeds are fixed, so
    /// the sketches are too.
    static func handDrawnStyles() -> Geometry {
        func element(_ type: String, _ x: Int, _ y: Int, _ seed: Int, _ extra: String) -> String {
            """
            {"id":"\(type)\(seed)","type":"\(type)","x":\(x),"y":\(y),"width":150,"height":100,"seed":\(seed)\(extra)}
            """
        }
        return geometry(
            of: """
                {"type":"excalidraw","version":2,"elements":[
                \([
                    element("rectangle", 20, 20, 11, #","strokeStyle":"dashed""#),
                    element("ellipse", 200, 20, 12, #","strokeStyle":"dotted","strokeWidth":4"#),
                    element("diamond", 20, 160, 13, #","roundness":{"type":2},"opacity":50,"strokeWidth":4"#),
                    element("rectangle", 200, 160, 14, #","roundness":{"type":3},"roughness":2"#),
                    element("ellipse", 20, 300, 15, #","roughness":0"#),
                    element("diamond", 200, 300, 16, ##","roughness":2,"strokeStyle":"dashed","strokeColor":"#e03131""##),
                ].joined(separator: ","))
                ]}
                """)
    }
}
