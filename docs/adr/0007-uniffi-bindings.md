# UniFFI bindings with a zero-copy geometry path

Swift (and later Kotlin) talk to the core through UniFFI-generated bindings. Element geometry for changed Elements bypasses UniFFI and is exposed as raw buffers; clients cache it and apply the viewport transform themselves, so panning and zooming never cross the FFI boundary.
