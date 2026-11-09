---
status: superseded by ADR-0008
---

# Core tessellates, clients render natively

The Rust core turns the Scene into triangle meshes; each client draws them with its own native GPU API (Metal on Apple, later Vulkan/GL elsewhere). Rendering the whole Canvas from Rust via wgpu was rejected: it would add several MB and take control away from the clients, which must stay as polished and platform-native as possible. The cost is one small renderer per platform. Glyph outlines from the bundled font are tessellated by the core too, so text renders identically everywhere.
