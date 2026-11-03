# WORKING-TITLE

A native, touch-first re-implementation of Excalidraw for Apple devices (later Android and Linux), storing everything locally.

## Language

### Content

**Drawing**:
One saved, named piece of work as listed in the Gallery; corresponds to one `.excalidraw` file.
_Avoid_: Board, document, canvas, file, sketch

**Scene**:
The Elements (and their attached files such as images) making up the open Drawing.
_Avoid_: Page, board

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

**Point handle**:
The grip on a point of a selected line or arrow (and a smaller one in the middle of each long segment) that is dragged to move the point or, from a middle, to add one; touch-sized for fingers. A tapped point handle selects the point, so that it can be deleted.
_Avoid_: Vertex, node, anchor

**Binding**:
The attachment of an arrow's end to a shape: the arrow stops short of the shape and follows it when it moves, resizes or turns. Made by putting an arrow's end on the shape, undone by putting it elsewhere or by moving the arrow without the shape. The shape an end is over is shown with a binding highlight.
_Avoid_: Connection, glue, snap

**Action Bar**:
The small floating bar shown next to a non-empty Selection offering actions such as duplicate, delete, z-order and group; replaces a context menu.
_Avoid_: Context menu, popup
