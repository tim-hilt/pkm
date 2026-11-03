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
        let editor = Editor.withExcalifont()
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

    /// A linear Element at (x, y) through `points`, which start at the origin.
    private static func linear(
        _ type: String, _ x: Int, _ y: Int, _ seed: Int, _ points: String, _ extra: String = ""
    ) -> String {
        """
        {"id":"\(type)\(seed)","type":"\(type)","x":\(x),"y":\(y),"width":150,"height":60,"seed":\(seed),\
        "points":\(points)\(extra)}
        """
    }

    /// Straight, multi-point and curved lines and arrows, closed and filled
    /// loops, rotated, dashed, and a selection of arrowheads.
    static func linesAndArrows() -> Geometry {
        let straight = "[[0,0],[150,40]]"
        let multi = "[[0,0],[50,50],[100,0],[150,50]]"
        let loop = "[[0,0],[100,0],[100,70],[0,70],[0,0]]"
        let round = #","roundness":{"type":2}"#
        func head(_ name: String, _ y: Int, _ seed: Int) -> String {
            linear("arrow", 230, y, seed, "[[0,0],[130,0]]", #","endArrowhead":"\#(name)""#)
        }
        return geometry(
            of: """
                {"type":"excalidraw","version":2,"elements":[
                \([
                    linear("line", 20, 20, 41, straight),
                    linear("line", 20, 100, 42, multi),
                    linear("line", 20, 180, 43, multi, round),
                    linear("line", 20, 260, 44, loop, ##","backgroundColor":"#a5d8ff","fillStyle":"hachure""##),
                    linear("arrow", 20, 360, 45, straight, #","startArrowhead":"triangle""#),
                    linear("arrow", 20, 430, 46, multi, round + #","strokeStyle":"dashed""#),
                    linear("arrow", 230, 380, 47, straight, #","angle":0.5,"strokeWidth":4"#),
                    head("arrow", 20, 51), head("bar", 60, 52), head("circle", 100, 53),
                    head("triangle_outline", 140, 54), head("diamond", 180, 55),
                    head("cardinality_one_or_many", 220, 56), head("cardinality_zero_or_one", 260, 57),
                    head("cardinality_exactly_one", 300, 58),
                ].joined(separator: ","))
                ]}
                """)
    }

    /// A freedraw stroke at (x, y) through `points`, which start at the origin.
    private static func freedraw(
        _ x: Int, _ y: Int, _ seed: Int, _ points: [(Double, Double)], pressures: [Double]? = nil,
        _ extra: String = ""
    ) -> String {
        func number(_ value: Double) -> String { String(format: "%.2f", value) }
        let path = points.map { "[\(number($0.0)),\(number($0.1))]" }.joined(separator: ",")
        var pressure = #","simulatePressure":true,"pressures":[]"#
        if let pressures {
            pressure = #","simulatePressure":false,"pressures":[\#(pressures.map(number).joined(separator: ","))]"#
        }
        return """
            {"id":"freedraw\(seed)","type":"freedraw","x":\(x),"y":\(y),"width":150,"height":60,"seed":\(seed),\
            "points":[\(path)]\(pressure)\(extra)}
            """
    }

    /// Strokes with simulated and with real pressure, thick, translucent,
    /// rotated, and closed loops with hachure and solid fills.
    static func freedraw() -> Geometry {
        let wave = (0..<24).map { (Double($0) * 6, 30 + 25 * sin(Double($0) / 3.5)) }
        let pressures = (0..<24).map { 0.25 + 0.6 * sin(Double($0) / 23 * .pi) }
        let loop = (0...24).map { (50 + 45 * cos(Double($0) / 24 * 2 * .pi), 50 + 45 * sin(Double($0) / 24 * 2 * .pi)) }
        let fill = ##","backgroundColor":"#ffc9c9""##
        return geometry(
            of: """
                {"type":"excalidraw","version":2,"elements":[
                \([
                    freedraw(20, 20, 61, wave),
                    freedraw(20, 100, 62, wave, pressures: pressures),
                    freedraw(20, 180, 63, wave, #","strokeWidth":4"#),
                    freedraw(20, 260, 64, wave, pressures: pressures, ##","strokeColor":"#e03131","opacity":50"##),
                    freedraw(200, 20, 65, wave, #","angle":0.5"#),
                    freedraw(200, 120, 66, loop, fill + #","fillStyle":"hachure""#),
                    freedraw(200, 240, 67, loop, pressures: loop.indices.map { _ in 0.5 }, fill + #","fillStyle":"solid""#),
                ].joined(separator: ","))
                ]}
                """)
    }

    /// A text Element at (x, y) with the given saved size; `extra` adds members.
    static func text(
        _ id: String, _ x: Int, _ y: Int, _ size: (Int, Int), _ text: String, _ extra: String = ""
    ) -> String {
        """
        {"id":"\(id)","type":"text","x":\(x),"y":\(y),"width":\(size.0),"height":\(size.1),\
        "text":"\(text)","originalText":"\(text)","fontSize":20,"fontFamily":5,"lineHeight":1.25\(extra)}
        """
    }

    /// A shape with the label `text` bound to it.
    private static func labelled(
        _ type: String, _ x: Int, _ y: Int, _ size: (Int, Int), _ text: String, _ extra: String = ""
    ) -> [String] {
        [
            """
            {"id":"\(type)","type":"\(type)","x":\(x),"y":\(y),"width":\(size.0),"height":\(size.1),\
            "roughness":0,"boundElements":[{"type":"text","id":"\(type)-label"}]}
            """,
            Self.text(
                "\(type)-label", x, y, (10, 25), text,
                #","containerId":"\#(type)","textAlign":"center","verticalAlign":"middle"\#(extra)"#),
        ]
    }

    /// Text in every alignment, size and font family, rotated, translucent,
    /// and labels wrapped to a rectangle, an ellipse and a diamond.
    static func text() -> Geometry {
        let label = "A label that wraps to its container"
        return geometry(
            of: """
                {"type":"excalidraw","version":2,"elements":[
                \((
                    [
                        text("left", 20, 20, (220, 25), "Left aligned", #","textAlign":"left""#),
                        text("centre", 20, 60, (220, 25), "Centred", #","textAlign":"center""#),
                        text("right", 20, 100, (220, 25), "Right aligned", #","textAlign":"right""#),
                        text("lines", 20, 140, (120, 60), "Two\\nlines", #","fontSize":24,"lineHeight":1.25"#),
                        text("family", 20, 220, (200, 23), "Another family", #","fontFamily":2,"lineHeight":1.15"#),
                        text(
                            "red", 20, 260, (220, 25), "Red, translucent",
                            ##","strokeColor":"#e03131","opacity":50"##),
                        text("turned", 40, 340, (160, 25), "Turned about its middle", #","angle":-0.3"#),
                    ]
                        + labelled("rectangle", 260, 20, (220, 110), label)
                        + labelled("ellipse", 260, 160, (220, 140), label)
                        + labelled("diamond", 260, 330, (220, 160), "A diamond label")
                ).joined(separator: ","))
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
