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

**Action Bar**:
The small floating bar shown next to a non-empty Selection offering actions such as duplicate, delete, z-order and group; replaces a context menu.
_Avoid_: Context menu, popup
