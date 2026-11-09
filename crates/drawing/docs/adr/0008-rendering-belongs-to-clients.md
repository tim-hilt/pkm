# Rendering belongs entirely to the clients

The core describes *what* to draw as resolution-independent geometry (e.g. the hand-drawn strokes generated from each Element's seed) and never tessellates, rasterises or lays out glyphs. Each client owns its whole rendering pipeline. Having the core tessellate (ADR-0005) was rejected because splitting the pipeline across the FFI boundary would take control away from the clients later and pull a tessellation dependency into the core; the price is more work in every client.
