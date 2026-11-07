/**
 * The stroke shader, a port of `StrokeShaders.metal` to WGSL with the same
 * stages. Keep the structs in sync with `CanvasRenderer.ts`.
 */
export const strokeShaders = /* wgsl */ `
struct FrameUniforms {
  // Scene point at the top-left corner of the view.
  origin: vec2f,
  // Render target size in device pixels.
  viewSize: vec2f,
  // Device pixels per Scene unit.
  scale: f32,
}

struct PartUniforms {
  // Premultiplied RGBA, element opacity already applied.
  color: vec4f,
  // Line width in Scene units.
  width: f32,
  // 1: draw only the pixels the stroke covers completely (the first of the
  // two passes that draw a translucent stroke without double blending).
  interiorOnly: f32,
  // Min x, min y, max x, max y of an upright rectangle in Scene coordinates
  // where nothing is drawn (an arrow's gap for its label); empty for none.
  hole: vec4f,
}

@group(0) @binding(0) var<uniform> frame: FrameUniforms;
@group(1) @binding(0) var<storage, read> segments: array<vec4f>;
@group(1) @binding(1) var<uniform> part: PartUniforms;

// Whether the device pixel centre \`pixel\` is in the part's hole.
fn inHole(pixel: vec2f) -> bool {
  let scene = pixel / frame.scale + frame.origin;
  return all(scene > part.hole.xy) && all(scene < part.hole.zw);
}

struct StrokeVertexOut {
  @builtin(position) position: vec4f,
  // Segment end points and half width, in device pixels.
  @location(0) @interpolate(flat) a: vec2f,
  @location(1) @interpolate(flat) b: vec2f,
  @location(2) @interpolate(flat) radius: f32,
}

// Width of the anti-aliasing ramp around the stroke edge, in device pixels.
const rampPixels = 1.0;

// One instance per segment; each is expanded into a quad that covers the
// segment's capsule (the segment grown by half the line width in every
// direction) plus the ramp. Overlapping capsules of neighbouring segments form
// the round joins.
@vertex
fn stroke_vertex(
  @builtin(vertex_index) vertexId: u32,
  @builtin(instance_index) segmentId: u32,
) -> StrokeVertexOut {
  let segment = segments[segmentId];
  let a = (segment.xy - frame.origin) * frame.scale;
  let b = (segment.zw - frame.origin) * frame.scale;
  let radius = part.width * frame.scale * 0.5;

  // Triangle strip corners: (u, v) = (0,-1), (0,1), (1,-1), (1,1).
  let u = f32(vertexId / 2u);
  let v = f32(vertexId % 2u) * 2.0 - 1.0;

  let along = b - a;
  let len = length(along);
  var direction = vec2f(1.0, 0.0);
  if (len > 1e-4) {
    direction = along / len;
  }
  let normal = vec2f(-direction.y, direction.x);
  let extent = radius + rampPixels;
  let pixel = mix(a, b, u) + direction * (u * 2.0 - 1.0) * extent + normal * v * extent;

  var out: StrokeVertexOut;
  out.position = vec4f(pixel.x / frame.viewSize.x * 2.0 - 1.0, 1.0 - pixel.y / frame.viewSize.y * 2.0, 0.0, 1.0);
  out.a = a;
  out.b = b;
  out.radius = radius;
  return out;
}

// Analytic anti-aliasing: coverage falls linearly from 1 to 0 across the one
// pixel straddling the capsule's edge.
@fragment
fn stroke_fragment(in: StrokeVertexOut) -> @location(0) vec4f {
  let p = in.position.xy;
  if (inHole(p)) {
    discard;
  }
  let ab = in.b - in.a;
  let t = clamp(dot(p - in.a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
  let distance = length(p - in.a - ab * t);
  var coverage = clamp(in.radius + 0.5 - distance, 0.0, 1.0);

  // Lines thinner than a pixel cannot reach full coverage.
  coverage *= min(1.0, in.radius * 2.0);
  if (part.interiorOnly > 0.5 && coverage < 1.0) {
    discard;
  }
  return part.color * coverage;
}
`;
