#include <metal_stdlib>
using namespace metal;

// Keep the structs in sync with CanvasRenderer.swift. Strokes first, then
// fills, then images.

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

// Fills (stencil-then-cover): the same two functions draw the fan triangles
// into the stencil buffer (colour writes off) and the cover rectangle that
// paints where the stencil says "inside". Vertices are Scene points.
struct FillVertexOut {
    float4 position [[position]];
};

vertex FillVertexOut fill_vertex(
    uint vertexId [[vertex_id]],
    constant float2 *points [[buffer(0)]],
    constant FrameUniforms &frame [[buffer(1)]]
) {
    float2 pixel = (points[vertexId] - frame.origin) * frame.scale;
    FillVertexOut out;
    out.position = float4(pixel.x / frame.viewSize.x * 2.0 - 1.0, 1.0 - pixel.y / frame.viewSize.y * 2.0, 0.0, 1.0);
    return out;
}

fragment float4 fill_fragment(constant PartUniforms &part [[buffer(2)]]) {
    return part.color;
}

// Images: one textured quad over the image's box. The box is mirrored and
// turned about its centre like Excalidraw does (`scale`, then `angle`), and
// the picture is the part of the texture from `uv.xy` to `uv.zw`.
struct ImageUniforms {
    // Top-left corner and size of the box in Scene units.
    float4 box;
    // Left, top, right, bottom of the shown part of the texture, 0...1.
    float4 uv;
    // 1 or -1 per axis.
    float2 scale;
    // Clockwise rotation in radians.
    float angle;
    float opacity;
    // Radius of the box's rounded corners in Scene units; 0 for sharp ones.
    float cornerRadius;
};

struct ImageVertexOut {
    float4 position [[position]];
    float2 uv;
    // Position in the box relative to its centre, before mirroring and turning.
    float2 local;
};

vertex ImageVertexOut image_vertex(
    uint vertexId [[vertex_id]],
    constant FrameUniforms &frame [[buffer(1)]],
    constant ImageUniforms &image [[buffer(2)]]
) {
    // Triangle strip corners: (0,0), (0,1), (1,0), (1,1).
    float2 corner = float2(vertexId / 2, vertexId % 2);
    float2 size = image.box.zw;
    float2 local = (corner - 0.5) * size;
    float2 offset = local * image.scale;
    float s = sin(image.angle);
    float c = cos(image.angle);
    float2 scene = image.box.xy + size * 0.5 + float2(offset.x * c - offset.y * s, offset.x * s + offset.y * c);
    float2 pixel = (scene - frame.origin) * frame.scale;

    ImageVertexOut out;
    out.position = float4(pixel.x / frame.viewSize.x * 2.0 - 1.0, 1.0 - pixel.y / frame.viewSize.y * 2.0, 0.0, 1.0);
    out.uv = mix(image.uv.xy, image.uv.zw, corner);
    out.local = local;
    return out;
}

fragment float4 image_fragment(
    ImageVertexOut in [[stage_in]],
    constant FrameUniforms &frame [[buffer(1)]],
    constant ImageUniforms &image [[buffer(2)]],
    texture2d<float> picture [[texture(0)]],
    sampler pictureSampler [[sampler(0)]]
) {
    // The texture holds premultiplied colours.
    float4 color = picture.sample(pictureSampler, in.uv) * image.opacity;
    if (image.cornerRadius > 0.0) {
        // Signed distance to the rounded box, turned into coverage over one
        // device pixel like the strokes' edges.
        float2 q = abs(in.local) - (image.box.zw * 0.5 - image.cornerRadius);
        float distance = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - image.cornerRadius;
        color *= clamp(0.5 - distance * frame.scale, 0.0, 1.0);
    }
    return color;
}
