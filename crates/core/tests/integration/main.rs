//! The core's integration tests, one module per area.
//!
//! They form a single test binary on purpose: every new test binary costs
//! linking time and, on macOS, a first-launch security scan of about 13 s.
//! Add new test files here as modules (`make lint` rejects files directly in
//! `tests/`). Run one area with `cargo test -p excalidraw-native-core --test integration select::`.

mod common;

mod arrow_binding;
mod arrow_labels;
mod benchmarks;
mod bound_text;
mod box_select;
mod clipboard;
mod context_menu;
mod cursor;
mod dark_mode;
mod draw_freedraw;
mod draw_lines;
mod draw_shapes;
mod edit_points;
mod eraser;
mod fills;
mod freedraw;
mod geometry;
mod geometry_cache;
mod golden;
mod group;
mod hand_drawn_shapes;
mod hand_tool;
mod hostile_values;
mod images;
mod insert_images;
mod json;
mod keyboard_shortcuts;
mod lines_and_arrows;
mod merge;
mod mouse_pan;
mod move_selection;
mod multi_touch;
mod navigation;
mod pencil_double_tap;
mod pencil_only;
mod png_export;
mod properties;
mod resize_rotate;
mod scene;
mod select;
mod selection_on_tool_change;
mod stable_handles;
mod text;
mod text_editing;
mod undo_redo;
mod z_order;
mod zoom_controls;
