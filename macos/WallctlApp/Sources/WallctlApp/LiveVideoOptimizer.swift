import CryptoKit
import Foundation

protocol LiveVideoTranscoding: Sendable {
    func frameRate(of source: URL) async throws -> Double
    func transcode(_ source: URL, to destination: URL, frameRate: Double) async throws
}

enum LiveVideoOptimizationPolicy {
    static let targetFrameRate = 30.0
    static let optimizesAbove = 60.0

    static func needsOptimization(sourceFrameRate: Double) -> Bool {
        sourceFrameRate.isFinite && sourceFrameRate > optimizesAbove
    }
}

struct FrameRateReducer {
    private static let tolerance = 0.0005

    private let interval: Double
    private var nextKeep: Double?

    init(frameRate: Double) {
        interval = 1 / frameRate
    }

    mutating func keeps(_ time: Double) -> Bool {
        guard time.isFinite else { return false }
        guard var next = nextKeep else {
            nextKeep = time + interval
            return true
        }
        guard time >= next - Self.tolerance else { return false }
        repeat { next += interval } while next <= time + Self.tolerance
        nextKeep = next
        return true
    }
}

struct LiveVideoOptimizer: Sendable {
    enum Outcome: Equatable, Sendable {
        case notNeeded
        case optimized(URL)
        case failed
    }

    private static let partialSuffix = ".partial.mov"

    let cacheDirectory: URL
    let maximumCacheBytes: Int64
    let transcoder: any LiveVideoTranscoding

    static let standard = LiveVideoOptimizer(
        cacheDirectory: FileManager.default
            .urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent(Bundle.main.bundleIdentifier ?? "wallctl", isDirectory: true)
            .appendingPathComponent("LivePlayback", isDirectory: true),
        maximumCacheBytes: 3 << 30,
        transcoder: AVFoundationVideoTranscoder()
    )

    func cachedURL(for source: URL) -> URL? {
        guard let url = cacheURL(for: source),
              FileManager.default.fileExists(atPath: url.path)
        else { return nil }
        try? FileManager.default.setAttributes([.modificationDate: Date()], ofItemAtPath: url.path)
        return url
    }

    func optimize(_ source: URL) async -> Outcome {
        if let cached = cachedURL(for: source) { return .optimized(cached) }
        guard let destination = cacheURL(for: source) else { return .failed }
        do {
            let frameRate = try await transcoder.frameRate(of: source)
            guard LiveVideoOptimizationPolicy.needsOptimization(sourceFrameRate: frameRate) else {
                return .notNeeded
            }
            try FileManager.default.createDirectory(
                at: cacheDirectory,
                withIntermediateDirectories: true
            )
            removePartialFiles()
            let partial = cacheDirectory.appendingPathComponent(
                destination.deletingPathExtension().lastPathComponent + Self.partialSuffix
            )
            try await transcoder.transcode(
                source,
                to: partial,
                frameRate: LiveVideoOptimizationPolicy.targetFrameRate
            )
            try FileManager.default.moveItem(at: partial, to: destination)
            prune(keeping: destination)
            return .optimized(destination)
        } catch {
            removePartialFiles()
            return .failed
        }
    }

    private func cacheURL(for source: URL) -> URL? {
        let resolved = source.standardizedFileURL.resolvingSymlinksInPath()
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: resolved.path),
              let size = attributes[.size] as? NSNumber,
              let modified = attributes[.modificationDate] as? Date
        else { return nil }
        let identity = [
            "v1",
            resolved.path,
            size.stringValue,
            String(modified.timeIntervalSince1970),
            String(LiveVideoOptimizationPolicy.targetFrameRate),
        ].joined(separator: "|")
        let digest = SHA256.hash(data: Data(identity.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return cacheDirectory.appendingPathComponent(String(digest.prefix(32)) + ".mov")
    }

    private func removePartialFiles() {
        let entries = (try? FileManager.default.contentsOfDirectory(
            at: cacheDirectory,
            includingPropertiesForKeys: nil
        )) ?? []
        for entry in entries where entry.lastPathComponent.hasSuffix(Self.partialSuffix) {
            try? FileManager.default.removeItem(at: entry)
        }
    }

    private func prune(keeping keep: URL) {
        let keys: [URLResourceKey] = [.fileSizeKey, .contentModificationDateKey]
        let entries = (try? FileManager.default.contentsOfDirectory(
            at: cacheDirectory,
            includingPropertiesForKeys: keys
        )) ?? []
        let files = entries.compactMap { url -> (url: URL, size: Int64, modified: Date)? in
            guard url.pathExtension == "mov",
                  !url.lastPathComponent.hasSuffix(Self.partialSuffix),
                  let values = try? url.resourceValues(forKeys: Set(keys)),
                  let size = values.fileSize,
                  let modified = values.contentModificationDate
            else { return nil }
            return (url, Int64(size), modified)
        }
        var total = files.reduce(0) { $0 + $1.size }
        for file in files.sorted(by: { $0.modified < $1.modified }) {
            guard total > maximumCacheBytes else { break }
            guard file.url.lastPathComponent != keep.lastPathComponent else { continue }
            try? FileManager.default.removeItem(at: file.url)
            total -= file.size
        }
    }
}
