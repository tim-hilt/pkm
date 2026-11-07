import Foundation
import Testing

@testable import ExcalidrawNative

/// The phone Action row's items and popover, with sections from a real controller.
@MainActor @Suite struct ActionRowModelTests {
    private static let drawing = """
        {"type":"excalidraw","version":2,"elements":[
        {"id":"box","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,\
        "strokeColor":"#1e1e1e","backgroundColor":"transparent","roundness":null,\
        "version":3,"versionNonce":7},
        {"id":"label","type":"text","x":300,"y":100,"width":60,"height":25,"text":"hi","fontSize":20,\
        "fontFamily":5,"textAlign":"left","verticalAlign":"top","originalText":"hi","autoResize":true,\
        "lineHeight":1.25,"strokeColor":"#1e1e1e","backgroundColor":"transparent","version":3,"versionNonce":9}]}
        """

    private let canvas = CanvasMetalView()
    private let editor = Editor()
    private let controller = CanvasController(settings: .isolated())

    private init() throws {
        _ = try #require(canvas.renderer)
        try editor.load(bytes: Data(Self.drawing.utf8))
        canvas.editor = editor
        controller.canvas = canvas
    }

    private func items(width: CGFloat = 365) -> [ActionRowItem] {
        ActionRowModel.items(
            sections: controller.properties?.sections, hasSelection: controller.hasSelection, width: width)
    }

    private func row(width: CGFloat = 365) -> ActionRowModel {
        var row = ActionRowModel()
        row.update(sections: controller.properties?.sections, hasSelection: controller.hasSelection, width: width)
        return row
    }

    @Test func aRectangleToolOffersEveryStyleTriggerButNoText() {
        controller.setTool(.rectangle)
        #expect(items() == [.stroke, .background, .properties])
    }

    @Test func anArrowHasNoBackgroundAndALineIsTheSame() {
        controller.setTool(.arrow)
        let arrow = items()
        #expect(!arrow.contains(.background) && arrow.contains(.stroke) && arrow.contains(.properties))
        controller.setTool(.line)
        #expect(items().contains(.properties))
    }

    @Test func freedrawHasNoTextTrigger() {
        controller.setTool(.freedraw)
        #expect(!items().contains(.text) && items().contains(.stroke))
    }

    @Test func textOffersStrokeTextAndPropertiesForItsOpacity() {
        controller.setTool(.text)
        #expect(items() == [.stroke, .properties, .text])
    }

    @Test func aMixedSelectionHasEveryTriggerThatAppliesToAnyElementAndActions() {
        controller.setTool(.selection)
        controller.perform(.selectAll)
        #expect(items() == [.stroke, .background, .properties, .text, .actions])
    }

    @Test func handSelectionAndEraserWithNothingSelectedHaveAnEmptyRow() {
        for tool in [Tool.hand, .selection, .eraser] {
            controller.setTool(tool)
            #expect(items().isEmpty, "\(tool)")
        }
    }

    @Test func deleteAndDuplicateMoveOntoTheRowByWidth() {
        controller.perform(.selectAll)
        #expect(items(width: 365).suffix(1) == [.actions])
        #expect(items(width: 374).suffix(2) == [.actions, .delete])
        #expect(items(width: 412).suffix(3) == [.actions, .delete, .duplicate])
        #expect(items(width: 450).suffix(3) == [.actions, .delete, .duplicate])
    }

    @Test func tappingOpensSwitchesAndCloses() {
        controller.setTool(.rectangle)
        var row = row()
        row.tap(.stroke)
        #expect(row.open == .stroke)
        row.tap(.properties)
        #expect(row.open == .properties)
        row.tap(.properties)
        #expect(row.open == nil)
    }

    @Test func aCanvasTouchClosesAndIsSwallowedOnlyWhenOpen() {
        controller.setTool(.rectangle)
        var row = row()
        let idle = row.canvasTouch()
        #expect(!idle)
        row.tap(.stroke)
        let swallowed = row.canvasTouch()
        #expect(swallowed)
        #expect(row.open == nil)
    }

    @Test func thePopoverClosesWhenItsTriggerGoesAndStaysWhenItRemains() {
        controller.perform(.selectAll)
        var row = row()
        row.tap(.actions)
        row.update(sections: controller.properties?.sections, hasSelection: true, width: 365)
        #expect(row.open == .actions)

        controller.deleteSelection()
        row.update(sections: controller.properties?.sections, hasSelection: controller.hasSelection, width: 365)
        #expect(row.open == nil)
    }
}
