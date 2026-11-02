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

    /// One 150 x 100 shape of `type` at (x, y) with a pastel background.
    private static func filled(
        _ type: String, _ x: Int, _ y: Int, _ seed: Int, fill: String, _ extra: String = ""
    ) -> String {
        """
        {"id":"\(type)\(seed)","type":"\(type)","x":\(x),"y":\(y),"width":150,"height":100,"seed":\(seed),\
        "backgroundColor":"#a5d8ff","fillStyle":"\(fill)"\(extra)}
        """
    }

    /// Every fill style on a rectangle.
    static func fillStyles() -> Geometry {
        geometry(
            of: """
                {"type":"excalidraw","version":2,"elements":[
                \([
                    filled("rectangle", 20, 20, 21, fill: "hachure"),
                    filled("rectangle", 200, 20, 22, fill: "cross-hatch"),
                    filled("rectangle", 20, 160, 23, fill: "zigzag"),
                    filled("rectangle", 200, 160, 24, fill: "solid"),
                    filled("rectangle", 20, 300, 25, fill: "hachure", #","roughness":0"#),
                    filled("rectangle", 200, 300, 26, fill: "cross-hatch", #","strokeWidth":4"#),
                ].joined(separator: ","))
                ]}
                """)
    }

    /// Fills on the other closed shapes, rounded, rotated and translucent.
    static func filledShapes() -> Geometry {
        geometry(
            of: """
                {"type":"excalidraw","version":2,"elements":[
                \([
                    filled("ellipse", 20, 20, 31, fill: "hachure"),
                    filled("ellipse", 200, 20, 32, fill: "solid"),
                    filled("diamond", 20, 160, 33, fill: "zigzag"),
                    filled("diamond", 200, 160, 34, fill: "solid", #","roundness":{"type":2}"#),
                    filled("rectangle", 20, 300, 35, fill: "cross-hatch", #","roundness":{"type":3},"angle":0.3"#),
                    filled("ellipse", 200, 300, 36, fill: "solid", ##","opacity":50,"strokeColor":"#e03131""##),
                ].joined(separator: ","))
                ]}
                """)
    }

    /// A concave outline and a pentagram, each by both fill rules.
    static func fillRules() -> HandBuiltGeometry {
        let ell: [SIMD2<Float>] = [[20, 20], [80, 20], [80, 120], [180, 120], [180, 180], [20, 180]]
        let star: [SIMD2<Float>] = (0..<5).map { i in
            let angle = Float(i) * 4 * .pi / 5 - .pi / 2
            return SIMD2(100 + 80 * cos(angle), 100 + 80 * sin(angle))
        }
        func moved(_ points: [SIMD2<Float>], by offset: SIMD2<Float>) -> [SIMD2<Float>] {
            points.map { $0 + offset }
        }
        return HandBuiltGeometry(
            .fill(ell), .fill(moved(ell, by: [200, 0]), rule: .evenOdd),
            .fill(moved(star, by: [0, 200]), color: 0xe031_31ff, rule: .nonZero),
            .fill(moved(star, by: [200, 200]), color: 0xe031_31ff, rule: .evenOdd))
    }
}
