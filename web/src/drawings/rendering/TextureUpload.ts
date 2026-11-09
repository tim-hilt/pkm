import { type DecodedImage, decodeImage } from "./ImageDecoder";

/** Makes the texture of the image file in `bytes`; `undefined` if it is no image. */
export type TextureLoader = (bytes: Uint8Array) => Promise<GPUTexture | undefined>;

const mipmapShader = /* wgsl */ `
struct Out {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
}

// One triangle covering the target.
@vertex
fn mipmap_vertex(@builtin(vertex_index) id: u32) -> Out {
  let corner = vec2f(f32((id << 1u) & 2u), f32(id & 2u));
  var out: Out;
  out.position = vec4f(corner * 2.0 - 1.0, 0.0, 1.0);
  out.uv = vec2f(corner.x, 1.0 - corner.y);
  return out;
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linearSampler: sampler;

@fragment
fn mipmap_fragment(in: Out) -> @location(0) vec4f {
  return textureSample(source, linearSampler, in.uv);
}
`;

const textureFormat: GPUTextureFormat = "rgba8unorm";

/**
 * Uploads decoded images to the GPU with a full mipmap chain, so zoomed-out
 * images stay smooth. WebGPU has no mipmap generation of its own: each level
 * is drawn from the one above with linear filtering, a 2x2 box filter.
 */
export class TextureUpload {
  private pipeline: GPURenderPipeline | undefined;
  private sampler: GPUSampler | undefined;

  constructor(private readonly device: GPUDevice) {}

  /** A loader for `ImageTextures`: decodes with the browser, then uploads. */
  readonly load: TextureLoader = async (bytes) => {
    const image = await decodeImage(bytes);
    if (!image) return undefined;
    try {
      return this.upload(image);
    } finally {
      image.bitmap.close();
    }
  };

  upload({ width, height, bitmap }: DecodedImage): GPUTexture {
    const levels = Math.floor(Math.log2(Math.max(width, height))) + 1;
    const texture = this.device.createTexture({
      size: [width, height],
      format: textureFormat,
      mipLevelCount: levels,
      usage:
        GPUTextureUsage.TEXTURE_BINDING |
        GPUTextureUsage.COPY_DST |
        GPUTextureUsage.RENDER_ATTACHMENT,
    });
    // The bitmap is premultiplied already, and so is what the shader expects.
    this.device.queue.copyExternalImageToTexture(
      { source: bitmap },
      { texture, premultipliedAlpha: true },
      [width, height],
    );
    if (levels > 1) this.generateMipmaps(texture, levels);
    return texture;
  }

  private generateMipmaps(texture: GPUTexture, levels: number): void {
    const { device } = this;
    this.pipeline ??= (() => {
      const module = device.createShaderModule({ code: mipmapShader });
      return device.createRenderPipeline({
        layout: "auto",
        vertex: { module, entryPoint: "mipmap_vertex" },
        fragment: {
          module,
          entryPoint: "mipmap_fragment",
          targets: [{ format: textureFormat }],
        },
      });
    })();
    this.sampler ??= device.createSampler({ minFilter: "linear", magFilter: "linear" });
    const encoder = device.createCommandEncoder();
    for (let level = 1; level < levels; level++) {
      const group = device.createBindGroup({
        layout: this.pipeline.getBindGroupLayout(0),
        entries: [
          {
            binding: 0,
            resource: texture.createView({ baseMipLevel: level - 1, mipLevelCount: 1 }),
          },
          { binding: 1, resource: this.sampler },
        ],
      });
      const pass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: texture.createView({ baseMipLevel: level, mipLevelCount: 1 }),
            loadOp: "clear",
            storeOp: "store",
          },
        ],
      });
      pass.setPipeline(this.pipeline);
      pass.setBindGroup(0, group);
      pass.draw(3);
      pass.end();
    }
    device.queue.submit([encoder.finish()]);
  }
}
