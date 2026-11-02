#include <metal_stdlib>
using namespace metal;

// Keep both structs in sync with CanvasRenderer.swift.

struct FrameUniforms {
    // Scene point at the top-left corner of the view.
    float2 origin;
    // Render target size in device pixels.
    float2 viewSize;
    // Device pixels per Scene unit.
    float scale;
};

struct PartUniforms {
    // Premultiplied RGBA, element opacity already applied.
    float4 color;
    // Line width in Scene units.
    float width;
    // 1: draw only the pixels the stroke covers completely (the first of the
    // two passes that draw a translucent stroke without double blending).
    float interiorOnly;
};

struct StrokeVertexOut {
    float4 position [[position]];
    // Segment end points and half width, in device pixels.
    float2 a [[flat]];
    float2 b [[flat]];
    float radius [[flat]];
};

// Width of the anti-aliasing ramp around the stroke edge, in device pixels.
constant float rampPixels = 1.0;

// One instance per segment; each is expanded into a quad that covers the
// segment's capsule (the segment grown by half the line width in every
// direction) plus the ramp. Overlapping capsules of neighbouring segments form
// the round joins.
vertex StrokeVertexOut stroke_vertex(
    uint vertexId [[vertex_id]],
    uint segmentId [[instance_id]],
    constant float4 *segments [[buffer(0)]],
    constant FrameUniforms &frame [[buffer(1)]],
    constant PartUniforms &part [[buffer(2)]]
) {
    float4 segment = segments[segmentId];
    float2 a = (segment.xy - frame.origin) * frame.scale;
    float2 b = (segment.zw - frame.origin) * frame.scale;
    float radius = part.width * frame.scale * 0.5;

    // Triangle strip corners: (u, v) = (0,-1), (0,1), (1,-1), (1,1).
    float u = float(vertexId / 2);
    float v = float(vertexId % 2) * 2.0 - 1.0;

    float2 along = b - a;
    float length = metal::length(along);
    float2 direction = length > 1e-4 ? along / length : float2(1.0, 0.0);
    float2 normal = float2(-direction.y, direction.x);
    float extent = radius + rampPixels;
    float2 pixel = mix(a, b, u) + direction * (u * 2.0 - 1.0) * extent + normal * v * extent;

    StrokeVertexOut out;
    out.position = float4(pixel.x / frame.viewSize.x * 2.0 - 1.0, 1.0 - pixel.y / frame.viewSize.y * 2.0, 0.0, 1.0);
    out.a = a;
    out.b = b;
    out.radius = radius;
    return out;
}

// Analytic anti-aliasing: coverage falls linearly from 1 to 0 across the one
// pixel straddling the capsule's edge.
fragment float4 stroke_fragment(
    StrokeVertexOut in [[stage_in]],
    constant PartUniforms &part [[buffer(2)]]
) {
    float2 p = in.position.xy;
    float2 ab = in.b - in.a;
    float t = clamp(dot(p - in.a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    float distance = length(p - in.a - ab * t);
    float coverage = clamp(in.radius + 0.5 - distance, 0.0, 1.0);

    // Lines thinner than a pixel cannot reach full coverage.
    coverage *= min(1.0, in.radius * 2.0);
    if (part.interiorOnly > 0.5 && coverage < 1.0) {
        discard_fragment();
    }
    return part.color * coverage;
}
