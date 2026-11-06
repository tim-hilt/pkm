# Excalidraw Native

A native, touch-first re-implementation of Excalidraw for Apple devices (later Android and Linux), storing everything locally.

## Language

### Content

**Drawing**:
One saved, named piece of work as listed in the Gallery; corresponds to one `.excalidraw` file.
_Avoid_: Board, document, canvas, file, sketch

**Scene**:
The Elements (and their attached files such as images) making up the open Drawing.
_Avoid_: Page, board

**Benchmark Scene**:
A generated Scene committed under `benchmarks/` that is used to measure the performance targets.
_Avoid_: Stress test, test drawing

**Element**:
A single shape, line, arrow, freedraw stroke, text or image within a Scene.
_Avoid_: Object, item, node, shape (when meaning any Element)

**Gallery**:
The overview of all Drawings on the device, from which a Drawing is opened, created or deleted.
_Avoid_: Library (Excalidraw uses that for reusable Element collections, which this app does not have), file browser

### Editing

**Canvas**:
The on-screen view in which the open Scene is displayed and edited.
_Avoid_: Using it for the Drawing or Scene itself

**Tool**:
The active mode that determines what the next input creates or does (e.g. rectangle, freedraw, eraser, selection, hand).
_Avoid_: Mode, brush

**Selection**:
The set of Elements currently selected for editing.
_Avoid_: Focus, highlight

**Box-select**:
Selecting by dragging a rectangle over empty Canvas with the selection Tool: the Selection becomes the Elements the rectangle fully encloses (shift: adds them to it).
_Avoid_: Marquee, lasso, rubber band

**Group**:
Elements that are selected, moved, duplicated and erased together: tapping one selects the outermost group it is in, and groups can be nested. A double-tap on an Element of a selected group goes into the group, where the Elements of the next group down (or single Elements) are selected on their own, until a tap outside it or on empty Canvas. Saved as Excalidraw's `groupIds`.
_Avoid_: Cluster, set

**Point handle**:
The grip on a point of a selected line or arrow (and a smaller one in the middle of each long segment) that is dragged to move the point or, from a middle, to add one; touch-sized for fingers. A tapped point handle selects the point, so that it can be deleted.
_Avoid_: Vertex, node, anchor

**Binding**:
The attachment of an arrow's end to a shape: the arrow stops short of the shape and follows it when it moves, resizes or turns. Made by putting an arrow's end on the shape, undone by putting it elsewhere or by moving the arrow without the shape. The shape an end is over is shown with a binding highlight.
_Avoid_: Connection, glue, snap

**Stroke**:
What the freedraw Tool draws from pointer down to pointer up: a freedraw Element that grows with the pointer and is drawn while it grows. A Pencil's stroke keeps its pressure; the stroke of a finger or mouse gets pressure made up from the speed of the pointer (simulated pressure). The Tool stays active after each stroke.
_Avoid_: Scribble (that is handwriting recognition), path, line (a different Element)

**Erase**:
What the eraser Tool does from pointer down to pointer up: the Elements the pointer's path touches are previewed faded while it is down and deleted together, as one undo step, when it lifts; a tap erases what is under it. An Element's group, label or container goes with it. The Tool stays active after each erase.
_Avoid_: Delete (that is the command for the Selection), rub out

**Text view**:
The native text view shown over the Canvas while a text is created or edited (like Excalidraw's textarea), at the position, size, zoom and style the core reports, so that the system keyboard, autocorrect, dictation and Scribble work. A tap with the text Tool opens it on empty Canvas (or on the text there), a double-tap on a text opens it on that text; committing keeps the text as one undo step, empty text is not kept, and a tap elsewhere commits.
_Avoid_: Text editor (that is the core's description of the view), textarea, input field

**Label**:
The text bound to a rectangle, diamond or ellipse (its container), as Excalidraw's bound text: made and edited in the Text view by a double-tap on the shape (or Enter on a selected shape on Mac), wrapped to the width the shape leaves it, centred in it, growing the shape when it does not fit, and moving, resizing, turning and erasing with it. Saved as a text Element with `containerId`, listed in the shape's `boundElements`.
_Avoid_: Caption, bound text (Excalidraw's name for it)

**Action Bar**:
The small floating bar shown next to a non-empty Selection offering actions such as duplicate, delete, z-order and group; replaces a context menu.
_Avoid_: Context menu, popup

**Properties panel**:
The panel that sets stroke colour, background colour, fill style, stroke width, stroke style, sloppiness, edges and opacity (and, for text, font size and text alignment) on the Selection, and, with an empty Selection, as the style of the next Element. It shows the Selection's current values, or none chosen where they are mixed.
_Avoid_: Inspector, style bar, sidebar
