import AVFoundation
import CoreMedia
import VideoToolbox

struct AVFoundationVideoTranscoder: LiveVideoTranscoding {
    enum Failure: Error {
        case noVideoTrack
        case cannotAddReaderOutput
        case cannotAddWriterInput
        case readerFailed
        case writerFailed
    }

    private static let bitsPerPixelPerFrame = 0.03

    func frameRate(of source: URL) async throws -> Double {
        let asset = AVURLAsset(url: source)
        guard let track = try await asset.loadTracks(withMediaType: .video).first else {
            throw Failure.noVideoTrack
        }
        return Double(try await track.load(.nominalFrameRate))
    }

    func transcode(_ source: URL, to destination: URL, frameRate: Double) async throws {
        let asset = AVURLAsset(url: source)
        guard let track = try await asset.loadTracks(withMediaType: .video).first else {
            throw Failure.noVideoTrack
        }
        let (size, transform, timeRange, formats) = try await track.load(
            .naturalSize, .preferredTransform, .timeRange, .formatDescriptions
        )

        var settings: [String: Any] = [
            AVVideoCodecKey: AVVideoCodecType.hevc,
            AVVideoWidthKey: Int(size.width),
            AVVideoHeightKey: Int(size.height),
            AVVideoCompressionPropertiesKey: [
                AVVideoProfileLevelKey: kVTProfileLevel_HEVC_Main10_AutoLevel as String,
                AVVideoAverageBitRateKey: Int(
                    size.width * size.height * frameRate * Self.bitsPerPixelPerFrame
                ),
                AVVideoExpectedSourceFrameRateKey: frameRate,
                AVVideoMaxKeyFrameIntervalKey: Int(frameRate * 2),
            ] as [String: Any],
        ]
        if let color = Self.colorProperties(of: formats.first) {
            settings[AVVideoColorPropertiesKey] = color
        }

        let job = try TranscodeJob(
            asset: asset,
            track: track,
            destination: destination,
            writerSettings: settings,
            transform: transform,
            startTime: timeRange.start,
            frameRate: frameRate
        )
        try await job.run()
    }

    private static func colorProperties(of format: CMFormatDescription?) -> [String: String]? {
        guard let format,
              let extensions = CMFormatDescriptionGetExtensions(format) as? [String: Any],
              let primaries = extensions[kCMFormatDescriptionExtension_ColorPrimaries as String] as? String,
              let transfer = extensions[kCMFormatDescriptionExtension_TransferFunction as String] as? String,
              let matrix = extensions[kCMFormatDescriptionExtension_YCbCrMatrix as String] as? String
        else { return nil }
        return [
            AVVideoColorPrimariesKey: primaries,
            AVVideoTransferFunctionKey: transfer,
            AVVideoYCbCrMatrixKey: matrix,
        ]
    }
}

private final class TranscodeJob: @unchecked Sendable {
    private let reader: AVAssetReader
    private let output: AVAssetReaderTrackOutput
    private let writer: AVAssetWriter
    private let input: AVAssetWriterInput
    private let startTime: CMTime
    private let queue = DispatchQueue(label: "local.wallctl.transcode", qos: .utility)
    private var reducer: FrameRateReducer
    private var continuation: CheckedContinuation<Void, any Error>?
    private var started = false
    private var cancelled = false
    private var finished = false

    init(
        asset: AVAsset,
        track: AVAssetTrack,
        destination: URL,
        writerSettings: [String: Any],
        transform: CGAffineTransform,
        startTime: CMTime,
        frameRate: Double
    ) throws {
        try? FileManager.default.removeItem(at: destination)
        reader = try AVAssetReader(asset: asset)
        output = AVAssetReaderTrackOutput(
            track: track,
            outputSettings: [
                kCVPixelBufferPixelFormatTypeKey as String:
                    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
                kCVPixelBufferIOSurfacePropertiesKey as String: [String: Any](),
            ]
        )
        output.alwaysCopiesSampleData = false
        guard reader.canAdd(output) else { throw AVFoundationVideoTranscoder.Failure.cannotAddReaderOutput }
        reader.add(output)

        writer = try AVAssetWriter(outputURL: destination, fileType: .mov)
        input = AVAssetWriterInput(mediaType: .video, outputSettings: writerSettings)
        input.expectsMediaDataInRealTime = false
        input.transform = transform
        guard writer.canAdd(input) else { throw AVFoundationVideoTranscoder.Failure.cannotAddWriterInput }
        writer.add(input)

        self.startTime = startTime
        reducer = FrameRateReducer(frameRate: frameRate)
    }

    func run() async throws {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
                queue.async { self.start(continuation) }
            }
        } onCancel: {
            queue.async { self.cancel() }
        }
    }

    private func start(_ continuation: CheckedContinuation<Void, any Error>) {
        self.continuation = continuation
        started = true
        guard !cancelled else { return abort(CancellationError()) }
        guard reader.startReading() else {
            return abort(reader.error ?? AVFoundationVideoTranscoder.Failure.readerFailed)
        }
        guard writer.startWriting() else {
            return abort(writer.error ?? AVFoundationVideoTranscoder.Failure.writerFailed)
        }
        writer.startSession(atSourceTime: startTime)
        input.requestMediaDataWhenReady(on: queue) { [self] in pump() }
    }

    private func cancel() {
        cancelled = true
        if started { abort(CancellationError()) }
    }

    private func pump() {
        while !finished, input.isReadyForMoreMediaData {
            if !autoreleasepool(invoking: pumpOne) { return }
        }
    }

    private func pumpOne() -> Bool {
        guard let sample = output.copyNextSampleBuffer() else {
            guard reader.status == .completed else {
                abort(reader.error ?? AVFoundationVideoTranscoder.Failure.readerFailed)
                return false
            }
            input.markAsFinished()
            writer.finishWriting { [self] in
                queue.async { self.complete() }
            }
            return false
        }
        let time = CMSampleBufferGetPresentationTimeStamp(sample).seconds
        guard reducer.keeps(time) else { return true }
        guard input.append(sample) else {
            abort(writer.error ?? AVFoundationVideoTranscoder.Failure.writerFailed)
            return false
        }
        return true
    }

    private func complete() {
        if writer.status == .completed {
            finish(.success(()))
        } else {
            finish(.failure(writer.error ?? AVFoundationVideoTranscoder.Failure.writerFailed))
        }
    }

    private func abort(_ error: any Error) {
        if reader.status == .reading { reader.cancelReading() }
        if writer.status == .writing { writer.cancelWriting() }
        finish(.failure(error))
    }

    private func finish(_ result: Result<Void, any Error>) {
        guard !finished else { return }
        finished = true
        continuation?.resume(with: result)
        continuation = nil
    }
}
