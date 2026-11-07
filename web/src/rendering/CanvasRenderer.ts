import {
  type GeometryBuffers,
  type GeometryPart,
  type GeometrySource,
  elementsInZOrder,
  holeOf,
  strokeKind,
} from "./GeometryBuffers";
import { strokeSegments } from "./StrokeFlattener";
import { strokeShaders } from "./StrokeShaders";

/** Where the Canvas looks at the Scene. */
export interface Viewport {
  /** Scene point shown at the top-left corner of the view. */
  originX: number;
  originY: number;
  /** View points per Scene unit. */
  zoom: number;
}

/**
 * Draws the core's geometry with WebGPU (ADR-0008, ADR-0015), a port of the
 * Swift client's Metal `CanvasRenderer`. Owns the whole pipeline: curve
 * flattening, GPU resources and the draw calls; it knows nothing about the DOM.
 *
 * Rendering is strictly on demand: a frame is produced only when the owner
 * asks for one, and a change of geometry or viewport only asks the owner to
 * ask (`onNeedsDisplay`); there is no render loop.
 *
 * Strokes are one instanced draw per part: every segment becomes a quad that
 * the vertex shader expands to a capsule (round caps and joins) and the
 * fragment shader anti-aliases analytically; 4x MSAA smooths what is left.
 *
 * A translucent stroke (hand-drawn shapes overlap themselves a lot) must not
 * blend twice where capsules overlap. It is drawn in three passes with the
 * stencil buffer: first everything that is fully covered, marking those
 * pixels so that no capsule paints over another, then the anti-aliased fringe
 * on pixels not marked, then once more without colour to clear the marks.
 *
 * What is drawn for an Element is cached per Element handle and version (and
 * flattening level), so a changed Scene only rebuilds the Elements that changed.
 *
 * Fills, text, images and the overlay follow with their own tickets.
 */
export class CanvasRenderer {
  /** Samples per pixel of the colour and stencil a frame is drawn into. */
  private static readonly sampleCount = 4;
  private static readonly stencilFormat: GPUTextureFormat = "stencil8";
  /** Largest tolerated flattening error, in device pixels. */
  private static readonly flatteningTolerancePixels = 0.1;

  /** Called when a new frame is needed. The owner decides when to draw it. */
  onNeedsDisplay: (() => void) | undefined;

  private viewportValue: Viewport = { originX: 0, originY: 0, zoom: 1 };
  private paperColorValue = 0xffffffff;
  private geometry: GeometrySource | undefined;
  /** Frames rendered so far; stays constant while nothing changes. */
  framesRendered = 0;

  private readonly frameLayout: GPUBindGroupLayout;
  private readonly strokeLayout: GPUBindGroupLayout;
  private readonly frameBuffer: GPUBuffer;
  private readonly frameGroup: GPUBindGroup;
  /** Stroke pipelines by stencil use: plain, interior, fringe, clear. */
  private readonly pipelines: Record<StencilUse, GPURenderPipeline>;
  private readonly uniformAlignment: number;

  /** Everything to draw, in z-order: the cached draws of all Elements. */
  private draws: StrokeDraw[] = [];
  private cache = new Map<number, CachedElement>();
  /** The `Geometry.generation` the cache belongs to. */
  private cacheGeneration: number | undefined;
  /** Power-of-two zoom level `draws` were flattened for. */
  private flatteningLevel: number | undefined;
  private drawsAreStale = true;
  /** Elements whose draws were built so far; a cache hit does not count. */
  elementsBuilt = 0;
  private frameTargets: { color: GPUTexture; stencil: GPUTexture } | undefined;

  constructor(
    private readonly device: GPUDevice,
    private readonly format: GPUTextureFormat,
  ) {
    this.uniformAlignment = device.limits.minUniformBufferOffsetAlignment;
    this.frameLayout = device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "uniform" },
        },
      ],
    });
    this.strokeLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX, buffer: { type: "read-only-storage" } },
        {
          binding: 1,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "uniform", hasDynamicOffset: true },
        },
      ],
    });
    this.frameBuffer = device.createBuffer({
      size: frameUniformsSize,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.frameGroup = device.createBindGroup({
      layout: this.frameLayout,
      entries: [{ binding: 0, resource: { buffer: this.frameBuffer } }],
    });

    const module = device.createShaderModule({ code: strokeShaders });
    const layout = device.createPipelineLayout({
      bindGroupLayouts: [this.frameLayout, this.strokeLayout],
    });
    const makePipeline = (compare: GPUCompareFunction, passOp: GPUStencilOperation) => {
      const stencil = { compare, passOp, failOp: "keep" as const };
      return device.createRenderPipeline({
        layout,
        vertex: { module, entryPoint: "stroke_vertex" },
        fragment: {
          module,
          entryPoint: "stroke_fragment",
          targets: [
            {
              format,
              // Premultiplied source-over.
              blend: {
                color: { srcFactor: "one", dstFactor: "one-minus-src-alpha" },
                alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha" },
              },
            },
          ],
        },
        primitive: { topology: "triangle-strip" },
        multisample: { count: CanvasRenderer.sampleCount },
        depthStencil: {
          format: CanvasRenderer.stencilFormat,
          stencilFront: stencil,
          stencilBack: stencil,
        },
      });
    };
    this.pipelines = {
      plain: makePipeline("always", "keep"),
      interior: makePipeline("not-equal", "replace"),
      fringe: makePipeline("not-equal", "keep"),
      clear: makePipeline("always", "zero"),
    };
  }

  get viewport(): Viewport {
    return this.viewportValue;
  }

  set viewport(viewport: Viewport) {
    const old = this.viewportValue;
    this.viewportValue = viewport;
    if (
      viewport.originX !== old.originX ||
      viewport.originY !== old.originY ||
      viewport.zoom !== old.zoom
    ) {
      this.onNeedsDisplay?.();
    }
  }

  /** The colour of the paper as `0xRRGGBBAA`: the Canvas colour the core reports. */
  get paperColor(): number {
    return this.paperColorValue;
  }

  set paperColor(color: number) {
    if (color !== this.paperColorValue) {
      this.paperColorValue = color;
      this.onNeedsDisplay?.();
    }
  }

  /**
   * Replaces what is drawn; the same object again is a no-op. Keeps `geometry`
   * around: curves are flattened from the core's buffers again whenever the
   * zoom level changes. Elements whose version is unchanged are not rebuilt.
   */
  setGeometry(geometry: GeometrySource): void {
    if (geometry === this.geometry) return;
    this.geometry = geometry;
    this.drawsAreStale = true;
    this.onNeedsDisplay?.();
  }

  /**
   * Renders one frame into `target` (a single-sampled 2D texture in the
   * renderer's format with render-attachment usage, such as the canvas'
   * current texture), cleared to the Canvas background first.
   *
   * @param pixelsPerPoint Device pixels per view point (the device pixel ratio).
   */
  renderFrame(target: GPUTexture, pixelsPerPoint: number): void {
    const { color, stencil } = this.targets(target.width, target.height);
    const { viewport } = this;
    const scale = viewport.zoom * pixelsPerPoint;
    this.refreshDraws(flatteningLevel(scale));

    this.device.queue.writeBuffer(
      this.frameBuffer,
      0,
      new Float32Array([
        viewport.originX,
        viewport.originY,
        target.width,
        target.height,
        scale,
        0,
        0,
        0,
      ]),
    );

    const encoder = this.device.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: color.createView(),
          resolveTarget: target.createView(),
          loadOp: "clear",
          storeOp: "discard",
          clearValue: clearColor(this.paperColorValue),
        },
      ],
      depthStencilAttachment: {
        view: stencil.createView(),
        stencilLoadOp: "clear",
        stencilStoreOp: "discard",
        stencilClearValue: 0,
      },
    });
    pass.setBindGroup(0, this.frameGroup);
    pass.setStencilReference(1);
    for (const stroke of this.draws) {
      if (stroke.isTranslucent) {
        this.draw(pass, stroke, "interior", 0);
        this.draw(pass, stroke, "fringe", 1);
        this.draw(pass, stroke, "clear", 2);
      } else {
        this.draw(pass, stroke, "plain", 0);
      }
    }
    pass.end();
    this.device.queue.submit([encoder.finish()]);
    this.framesRendered++;
  }

  /** Draws `stroke` with the uniforms in slot `slot` and the stencil use `use`. */
  private draw(pass: GPURenderPassEncoder, stroke: StrokeDraw, use: StencilUse, slot: number) {
    pass.setPipeline(this.pipelines[use]);
    pass.setBindGroup(1, stroke.group, [slot * this.uniformAlignment]);
    pass.draw(4, stroke.segmentCount);
  }

  /**
   * The multisampled colour and stencil for a frame of `width` x `height`
   * pixels, kept while the size stays.
   */
  private targets(width: number, height: number) {
    const current = this.frameTargets;
    if (current && current.color.width === width && current.color.height === height) {
      return current;
    }
    current?.color.destroy();
    current?.stencil.destroy();
    const texture = (format: GPUTextureFormat) =>
      this.device.createTexture({
        size: [width, height],
        sampleCount: CanvasRenderer.sampleCount,
        format,
        usage: GPUTextureUsage.RENDER_ATTACHMENT,
      });
    this.frameTargets = {
      color: texture(this.format),
      stencil: texture(CanvasRenderer.stencilFormat),
    };
    return this.frameTargets;
  }

  /**
   * Brings `draws` up to date with the geometry and the zoom: rebuilds the
   * Elements that changed or were flattened for another power-of-two zoom
   * level, reuses the others.
   */
  private refreshDraws(level: number): void {
    const geometry = this.geometry;
    if (!geometry || !(this.drawsAreStale || level !== this.flatteningLevel)) return;
    this.drawsAreStale = false;
    this.flatteningLevel = level;
    if (geometry.generation() !== this.cacheGeneration) {
      // A Drawing was loaded: handles mean other Elements now.
      this.destroyCache();
      this.cacheGeneration = geometry.generation();
    }
    const tolerance = CanvasRenderer.flatteningTolerancePixels / 2 ** level;

    const kept = new Map<number, CachedElement>();
    const all: StrokeDraw[] = [];
    geometry.withBuffers((buffers) => {
      for (const item of elementsInZOrder(buffers)) {
        let entry = this.cache.get(item.element);
        if (entry?.version !== item.version || entry.level !== level) {
          entry?.draws.forEach((draw) => draw.destroy());
          const draws: StrokeDraw[] = [];
          for (let index = item.start; index < item.end; index++) {
            const draw = this.makeDraw(buffers.parts.at(index), buffers, tolerance);
            if (draw) draws.push(draw);
          }
          entry = { version: item.version, level, draws };
          this.elementsBuilt++;
        }
        kept.set(item.element, entry);
        all.push(...entry.draws);
      }
    });
    // Elements that are gone give up their buffers.
    for (const [element, entry] of this.cache) {
      if (!kept.has(element)) entry.draws.forEach((draw) => draw.destroy());
    }
    this.cache = kept;
    this.draws = all;
  }

  private destroyCache(): void {
    for (const entry of this.cache.values()) entry.draws.forEach((draw) => draw.destroy());
    this.cache.clear();
  }

  private makeDraw(
    part: GeometryPart,
    buffers: GeometryBuffers,
    tolerance: number,
  ): StrokeDraw | undefined {
    // Fills come with their own ticket.
    if (part.kind !== strokeKind) return undefined;
    const segments = strokeSegments(part, buffers, tolerance);
    if (segments.length === 0) return undefined;

    const color = premultipliedColor(part.color, part.opacity);
    const isTranslucent = color[3] < 1;
    const hole = holeOf(part);
    // One uniform slot per way the part is drawn, each at its own aligned
    // offset: see `draw`. Translucent: interior only, fringe, clear.
    const slots = isTranslucent
      ? [
          partUniforms(color, part.width, 1, hole),
          partUniforms(color, part.width, 0, hole),
          partUniforms([0, 0, 0, 0], part.width, 0, hole),
        ]
      : [partUniforms(color, part.width, 0, hole)];
    const uniforms = this.device.createBuffer({
      size: slots.length * this.uniformAlignment,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    slots.forEach((slot, index) =>
      this.device.queue.writeBuffer(uniforms, index * this.uniformAlignment, slot),
    );
    const segmentBuffer = this.device.createBuffer({
      size: segments.byteLength,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
    this.device.queue.writeBuffer(segmentBuffer, 0, segments);
    const group = this.device.createBindGroup({
      layout: this.strokeLayout,
      entries: [
        { binding: 0, resource: { buffer: segmentBuffer } },
        { binding: 1, resource: { buffer: uniforms, size: partUniformsSize } },
      ],
    });
    return {
      group,
      segmentCount: segments.length / 4,
      isTranslucent,
      destroy: () => {
        segmentBuffer.destroy();
        uniforms.destroy();
      },
    };
  }
}

type StencilUse = "plain" | "interior" | "fringe" | "clear";

/** A stroked part's segments on the GPU plus how to draw them. */
interface StrokeDraw {
  group: GPUBindGroup;
  segmentCount: number;
  /** Whether the stroke needs the three-pass treatment. */
  isTranslucent: boolean;
  destroy(): void;
}

/** The draws of one Element at the version and flattening level they were built for. */
interface CachedElement {
  version: number;
  level: number;
  draws: StrokeDraw[];
}

/** Mirrors `FrameUniforms` in StrokeShaders.ts, padded to 32 bytes. */
const frameUniformsSize = 32;
/** Mirrors `PartUniforms` in StrokeShaders.ts. */
const partUniformsSize = 48;

function partUniforms(
  color: readonly number[],
  width: number,
  interiorOnly: number,
  hole: readonly number[],
): Float32Array {
  return new Float32Array([...color, width, interiorOnly, 0, 0, ...hole]);
}

/** `rgba` (`0xRRGGBBAA`) as the colour to clear to. */
function clearColor(rgba: number): GPUColor {
  const channel = (shift: number) => ((rgba >>> shift) & 0xff) / 255;
  return { r: channel(24), g: channel(16), b: channel(8), a: channel(0) };
}

/** The power of two that curves are flattened for at `scale` device pixels per Scene unit. */
function flatteningLevel(scale: number): number {
  return Math.ceil(Math.log2(Math.max(scale, Number.MIN_VALUE)));
}

/** Premultiplied RGBA of `color` (`0xRRGGBBAA`) at `opacity`. */
function premultipliedColor(color: number, opacity: number): [number, number, number, number] {
  const channel = (shift: number) => ((color >>> shift) & 0xff) / 255;
  const alpha = channel(0) * opacity;
  return [channel(24) * alpha, channel(16) * alpha, channel(8) * alpha, alpha];
}
