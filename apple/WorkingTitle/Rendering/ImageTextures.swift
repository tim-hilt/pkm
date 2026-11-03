import Foundation
import Metal

/// Where the renderer gets the bytes of image files from: the core's `Editor`.
protocol ImageFileSource: AnyObject {
    /// The stored bytes of file `id`, or nil if there is no such file.
    func fileBytes(id: String) -> Data?
}

extension Editor: ImageFileSource {}

/// Textures of image files, one per file id however many Elements show it.
///
/// A texture is made the first time a file is asked for: its bytes come from
/// the `ImageFileSource`, `ImageDecoder` decodes them and the pixels go to the
/// GPU with a full mipmap chain, so zoomed-out images stay smooth. A file that
/// is missing or cannot be decoded is remembered as such, so that asking
/// again does not repeat the work. `keepOnly` releases what nothing refers to
/// any more.
@MainActor
final class ImageTextures {
    private enum Entry {
        case texture(any MTLTexture)
        /// The Drawing has no bytes for the file.
        case missing
        /// The file is there but is not an image ImageIO can read.
        case undecodable
    }

    private let device: any MTLDevice
    private let queue: any MTLCommandQueue
    private var entries: [String: Entry] = [:]

    /// Files decoded so far, successfully or not; tests use it to see what is
    /// cached. Missing files are not counted.
    private(set) var decodeCount = 0

    init(device: any MTLDevice, queue: any MTLCommandQueue) {
        self.device = device
        self.queue = queue
    }

    /// Number of textures held.
    var count: Int {
        entries.values.filter { if case .texture = $0 { true } else { false } }.count
    }

    /// The texture of file `id`; nil if there is none because the file is
    /// missing or is not an image.
    func texture(forFile id: String, from files: (any ImageFileSource)?) -> (any MTLTexture)? {
        switch entries[id] {
        case .texture(let texture)?: return texture
        case .missing?, .undecodable?: return nil
        case nil: break
        }
        guard let bytes = files?.fileBytes(id: id) else {
            entries[id] = .missing
            return nil
        }
        decodeCount += 1
        guard let texture = ImageDecoder.decode(bytes).flatMap(makeTexture) else {
            entries[id] = .undecodable
            return nil
        }
        entries[id] = .texture(texture)
        return texture
    }

    /// Whether the file is there but could not be decoded, as of the last time
    /// it was asked for.
    func isUndecodable(_ id: String) -> Bool {
        if case .undecodable? = entries[id] { true } else { false }
    }

    /// Releases the textures of all files not in `referenced`.
    func keepOnly(_ referenced: Set<String>) {
        entries = entries.filter { referenced.contains($0.key) }
    }

    /// Forgets which files were unusable. A Drawing that was loaded since may
    /// have them.
    func forgetFailures() {
        entries = entries.filter { if case .texture = $0.value { true } else { false } }
    }

    private func makeTexture(_ image: DecodedImage) -> (any MTLTexture)? {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .rgba8Unorm, width: image.width, height: image.height, mipmapped: true)
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
        image.pixels.withUnsafeBytes {
            texture.replace(
                region: MTLRegionMake2D(0, 0, image.width, image.height), mipmapLevel: 0,
                withBytes: $0.baseAddress!, bytesPerRow: image.width * 4)
        }
        if texture.mipmapLevelCount > 1, let commands = queue.makeCommandBuffer(),
            let blit = commands.makeBlitCommandEncoder()
        {
            blit.generateMipmaps(for: texture)
            blit.endEncoding()
            commands.commit()
        }
        return texture
    }
}
