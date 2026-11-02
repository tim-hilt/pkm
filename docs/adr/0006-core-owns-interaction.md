# Core owns all interaction logic

Clients forward raw pointer events (position, finger/Pencil/mouse, pressure, timestamp) to the Rust core, which owns Tool state, Selection, handles, undo history and gesture recognition (tap vs. drag vs. pan/pinch). The touch behaviour is the reason this app exists, so it must behave identically on every platform and be testable from recorded input sequences, even though leaving gesture recognition to each platform would be more conventional.
