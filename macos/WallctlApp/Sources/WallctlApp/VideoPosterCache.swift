import AppKit
import AVFoundation

@MainActor
final class VideoPosterCache {
    static let shared = VideoPosterCache()

    private let images = NSCache<NSURL, NSImage>()

    private init() {
        images.countLimit = 16
        images.totalCostLimit = 24 * 1_024 * 1_024
    }

    func removeAllImages() {
        images.removeAllObjects()
    }

    func image(for url: URL) async -> NSImage? {
        if let cached = images.object(forKey: url as NSURL) {
            return cached
        }

        let asset = AVURLAsset(url: url)
        let duration = try? await asset.load(.duration)
        let durationSeconds = duration?.seconds ?? 0
        let posterSecond = durationSeconds.isFinite && durationSeconds > 0
            ? min(max(durationSeconds * 0.02, 0.5), 2)
            : 1
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 1_024, height: 576)
        generator.requestedTimeToleranceBefore = CMTime(seconds: 0.25, preferredTimescale: 600)
        generator.requestedTimeToleranceAfter = CMTime(seconds: 0.25, preferredTimescale: 600)

        guard let generated = try? await generator.image(
            at: CMTime(seconds: posterSecond, preferredTimescale: 600)
        ) else {
            return nil
        }
        let image = NSImage(cgImage: generated.image, size: .zero)
        let cost = generated.image.bytesPerRow * generated.image.height
        images.setObject(image, forKey: url as NSURL, cost: cost)
        return image
    }
}
