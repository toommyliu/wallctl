import XCTest
@testable import WallctlApp

final class LiveVideoOptimizerTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: directory)
    }

    func testReducerKeepsEveryEighthFrameFrom240To30() {
        var reducer = FrameRateReducer(frameRate: 30)
        let kept = (0..<2400)
            .map { Double($0) / 240 }
            .filter { reducer.keeps($0) }

        XCTAssertEqual(kept.count, 300)
        XCTAssertEqual(kept[1], 8.0 / 240, accuracy: 1e-9)
    }

    func testReducerKeepsEveryFrameAtTargetRate() {
        var reducer = FrameRateReducer(frameRate: 30)
        let kept = (0..<300).map { Double($0) / 30 }.filter { reducer.keeps($0) }

        XCTAssertEqual(kept.count, 300)
    }

    func testReducerDoesNotBurstAfterATimestampGap() {
        var reducer = FrameRateReducer(frameRate: 30)
        let kept = [0, 0.5, 0.51, 0.52, 0.54].filter { reducer.keeps($0) }

        XCTAssertEqual(kept, [0, 0.5, 0.54])
    }

    func testReducerDropsNonFiniteTimestamps() {
        var reducer = FrameRateReducer(frameRate: 30)

        XCTAssertFalse(reducer.keeps(.nan))
        XCTAssertTrue(reducer.keeps(0))
    }

    func testOnlyFrameRatesAboveSixtyNeedOptimizing() {
        XCTAssertTrue(LiveVideoOptimizationPolicy.needsOptimization(sourceFrameRate: 240))
        XCTAssertFalse(LiveVideoOptimizationPolicy.needsOptimization(sourceFrameRate: 60))
        XCTAssertFalse(LiveVideoOptimizationPolicy.needsOptimization(sourceFrameRate: 30))
        XCTAssertFalse(LiveVideoOptimizationPolicy.needsOptimization(sourceFrameRate: .nan))
    }

    func testHighFrameRateSourceIsTranscodedOnceAndReused() async throws {
        let source = try makeSource("night.mov", contents: "original")
        let transcoder = FakeTranscoder(frameRate: 240)
        let optimizer = makeOptimizer(transcoder)

        XCTAssertNil(optimizer.cachedURL(for: source))
        let first = await optimizer.optimize(source)
        let second = await optimizer.optimize(source)

        guard case .optimized(let url) = first else { return XCTFail("\(first)") }
        XCTAssertEqual(second, .optimized(url))
        XCTAssertEqual(optimizer.cachedURL(for: source), url)
        XCTAssertEqual(url.deletingLastPathComponent().standardizedFileURL, cacheDirectory.standardizedFileURL)
        XCTAssertEqual(transcoder.transcodes, [30])
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
    }

    func testLowFrameRateSourceIsLeftAlone() async throws {
        let source = try makeSource("day.mov", contents: "original")
        let transcoder = FakeTranscoder(frameRate: 30)
        let optimizer = makeOptimizer(transcoder)

        let outcome = await optimizer.optimize(source)

        XCTAssertEqual(outcome, .notNeeded)
        XCTAssertTrue(transcoder.transcodes.isEmpty)
        XCTAssertNil(optimizer.cachedURL(for: source))
    }

    func testFailedTranscodeLeavesNoFiles() async throws {
        let source = try makeSource("night.mov", contents: "original")
        let transcoder = FakeTranscoder(frameRate: 240, failsAfterWriting: true)
        let optimizer = makeOptimizer(transcoder)

        let outcome = await optimizer.optimize(source)

        XCTAssertEqual(outcome, .failed)
        XCTAssertNil(optimizer.cachedURL(for: source))
        XCTAssertEqual(try cacheContents(), [])
    }

    func testCancelledTranscodeLeavesNoFiles() async throws {
        let source = try makeSource("night.mov", contents: "original")
        let transcoder = FakeTranscoder(frameRate: 240, blocksUntilCancelled: true)
        let optimizer = makeOptimizer(transcoder)

        let task = Task { await optimizer.optimize(source) }
        try await Task.sleep(for: .milliseconds(100))
        task.cancel()
        let outcome = await task.value

        XCTAssertEqual(outcome, .failed)
        XCTAssertEqual(try cacheContents(), [])
    }

    func testChangedSourceGetsANewCacheEntry() async throws {
        let source = try makeSource("night.mov", contents: "original")
        let optimizer = makeOptimizer(FakeTranscoder(frameRate: 240))

        let first = await optimizer.optimize(source)
        try Data("replacement with a different size".utf8).write(to: source)
        let second = await optimizer.optimize(source)

        guard case .optimized(let firstURL) = first,
              case .optimized(let secondURL) = second
        else { return XCTFail("\(first) \(second)") }
        XCTAssertNotEqual(firstURL, secondURL)
    }

    func testCacheEvictsOldestEntriesOverTheLimit() async throws {
        let transcoder = FakeTranscoder(frameRate: 240, outputBytes: 100)
        let optimizer = makeOptimizer(transcoder, maximumCacheBytes: 150)
        let sources = try ["a", "b", "c"].map { try makeSource("\($0).mov", contents: $0) }

        for source in sources { _ = await optimizer.optimize(source) }

        XCTAssertNil(optimizer.cachedURL(for: sources[0]))
        XCTAssertNil(optimizer.cachedURL(for: sources[1]))
        XCTAssertNotNil(optimizer.cachedURL(for: sources[2]))
    }

    private var cacheDirectory: URL {
        directory.appendingPathComponent("cache", isDirectory: true)
    }

    private func makeOptimizer(
        _ transcoder: FakeTranscoder,
        maximumCacheBytes: Int64 = 1 << 30
    ) -> LiveVideoOptimizer {
        LiveVideoOptimizer(
            cacheDirectory: cacheDirectory,
            maximumCacheBytes: maximumCacheBytes,
            transcoder: transcoder
        )
    }

    private func makeSource(_ name: String, contents: String) throws -> URL {
        let url = directory.appendingPathComponent(name)
        try Data(contents.utf8).write(to: url)
        return url
    }

    private func cacheContents() throws -> [String] {
        guard FileManager.default.fileExists(atPath: cacheDirectory.path) else { return [] }
        return try FileManager.default.contentsOfDirectory(atPath: cacheDirectory.path)
    }
}

private final class FakeTranscoder: LiveVideoTranscoding, @unchecked Sendable {
    private let lock = NSLock()
    private var recorded = [Double]()
    private let reportedFrameRate: Double
    private let failsAfterWriting: Bool
    private let blocksUntilCancelled: Bool
    private let outputBytes: Int

    init(
        frameRate: Double,
        failsAfterWriting: Bool = false,
        blocksUntilCancelled: Bool = false,
        outputBytes: Int = 8
    ) {
        reportedFrameRate = frameRate
        self.failsAfterWriting = failsAfterWriting
        self.blocksUntilCancelled = blocksUntilCancelled
        self.outputBytes = outputBytes
    }

    var transcodes: [Double] {
        lock.withLock { recorded }
    }

    func frameRate(of source: URL) async throws -> Double {
        reportedFrameRate
    }

    func transcode(_ source: URL, to destination: URL, frameRate: Double) async throws {
        let count = lock.withLock {
            recorded.append(frameRate)
            return recorded.count
        }
        try Data(repeating: 1, count: outputBytes).write(to: destination)
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSince1970: 1_000 + Double(count))],
            ofItemAtPath: destination.path
        )
        if blocksUntilCancelled { try await Task.sleep(for: .seconds(60)) }
        if failsAfterWriting { throw CocoaError(.fileWriteUnknown) }
    }
}
