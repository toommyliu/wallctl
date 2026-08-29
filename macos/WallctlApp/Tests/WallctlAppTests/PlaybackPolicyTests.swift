import Combine
import XCTest
@testable import WallctlApp

final class PlaybackPolicyTests: XCTestCase {
    @MainActor
    func testPowerStateNotificationCanArriveOffMainThread() async {
        let controller = LiveWallpaperController()

        await Task.detached {
            NotificationCenter.default.post(
                name: .NSProcessInfoPowerStateDidChange,
                object: nil
            )
        }.value
        await Task.yield()

        withExtendedLifetime(controller) {}
    }

    func testLiveWallpaperMenuToggleReenablesAfterTurningPlaybackOff() {
        let enabled = LiveConfig(
            enabled: true,
            followActiveCollection: true,
            pinnedCollection: nil,
            pauseOnBattery: true
        )
        let disabled = LiveConfig(
            enabled: false,
            followActiveCollection: true,
            pinnedCollection: nil,
            pauseOnBattery: true
        )

        let source = LiveWallpaperMenuStateSource(config: enabled)
        var states = [LiveWallpaperMenuState]()
        let subscription = LiveWallpaperMenuState.updates(
            source.$config,
            source.$isWorking,
            source.$constraints,
            source.$activeVideo
        )
        .sink { states.append($0) }

        source.isWorking = true
        source.config = disabled
        source.isWorking = false

        XCTAssertTrue(states.first?.isToggleEnabled == true)
        XCTAssertTrue(states.first?.isLiveEnabled == true)
        XCTAssertTrue(states.contains {
            !$0.isToggleEnabled && !$0.isLiveEnabled
        })
        XCTAssertTrue(states.last?.isToggleEnabled == true)
        XCTAssertTrue(states.last?.isLiveEnabled == false)
        XCTAssertEqual(states.last?.status, "Off")
        withExtendedLifetime(subscription) {}
    }

    func testPlaybackRequiresEveryConstraintToBeClear() {
        XCTAssertTrue(PlaybackConstraints().shouldPlay)

        var constraints = PlaybackConstraints()
        constraints.lowPowerMode = true
        XCTAssertFalse(constraints.shouldPlay)
        XCTAssertEqual(constraints.pauseReason, "Low Power Mode is on")
    }

    func testBatteryConstraintIsOptional() {
        var constraints = PlaybackConstraints()
        constraints.onBattery = true
        constraints.pauseOnBattery = false
        XCTAssertTrue(constraints.shouldPlay)
    }

    func testAppearanceChangeKeepsAppearanceIndependentVideo() {
        let light = LiveVideoSource.resolvedPath(
            videoPath: "/wallpapers/day.mov",
            lightVideoPath: nil,
            darkVideoPath: nil,
            automatic: true,
            prefersDark: false
        )
        let dark = LiveVideoSource.resolvedPath(
            videoPath: "/wallpapers/day.mov",
            lightVideoPath: nil,
            darkVideoPath: nil,
            automatic: true,
            prefersDark: true
        )

        XCTAssertEqual(light, dark)
    }

    func testAppearanceChangeSelectsRealVideoVariant() {
        let light = LiveVideoSource.resolvedPath(
            videoPath: "/wallpapers/fallback.mov",
            lightVideoPath: "/wallpapers/day.mov",
            darkVideoPath: "/wallpapers/night.mov",
            automatic: true,
            prefersDark: false
        )
        let dark = LiveVideoSource.resolvedPath(
            videoPath: "/wallpapers/fallback.mov",
            lightVideoPath: "/wallpapers/day.mov",
            darkVideoPath: "/wallpapers/night.mov",
            automatic: true,
            prefersDark: true
        )

        XCTAssertEqual(light, "/wallpapers/day.mov")
        XCTAssertEqual(dark, "/wallpapers/night.mov")
    }

    func testEquivalentVideoURLsResolveSymlinks() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let video = directory.appendingPathComponent("day.mov")
        let link = directory.appendingPathComponent("current.mov")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        XCTAssertTrue(FileManager.default.createFile(atPath: video.path, contents: Data()))
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: video)

        XCTAssertTrue(LiveVideoSource.equivalent(video, link))
    }

    func testLoopTransitionBeginsInsideLeadTime() {
        XCTAssertFalse(LoopTransitionPolicy.shouldBegin(
            currentTime: 118,
            duration: 120,
            leadTime: 1.5
        ))
        XCTAssertTrue(LoopTransitionPolicy.shouldBegin(
            currentTime: 118.5,
            duration: 120,
            leadTime: 1.5
        ))
        XCTAssertTrue(LoopTransitionPolicy.shouldBegin(
            currentTime: 119.9,
            duration: 120,
            leadTime: 1.5
        ))
    }

    func testLoopTransitionRejectsInvalidTiming() {
        XCTAssertFalse(LoopTransitionPolicy.shouldBegin(
            currentTime: .nan,
            duration: 120,
            leadTime: 1.5
        ))
        XCTAssertFalse(LoopTransitionPolicy.shouldBegin(
            currentTime: 0.5,
            duration: 1,
            leadTime: 1.5
        ))
    }

    func testScheduleTimingWrapsAndFindsNextBoundary() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(secondsFromGMT: 0))
        let morning = ScheduleSlot(hour: 6, profile: "morning")
        let evening = ScheduleSlot(hour: 17, profile: "evening")
        let late = try XCTUnwrap(calendar.date(from: DateComponents(
            year: 2026,
            month: 7,
            day: 10,
            hour: 20
        )))

        XCTAssertEqual(
            ScheduleTiming.currentProfile(slots: [morning, evening], at: late, calendar: calendar),
            "evening"
        )
        let next = try XCTUnwrap(ScheduleTiming.nextBoundary(
            hours: [6, 17],
            after: late,
            calendar: calendar
        ))
        XCTAssertEqual(calendar.component(.day, from: next), 11)
        XCTAssertEqual(calendar.component(.hour, from: next), 6)
    }

    func testApplyPolicyConfirmsOnlyOutsideTheCurrentScheduleSlot() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(secondsFromGMT: 0))
        let morning = ScheduleSlot(hour: 6, profile: "morning")
        let day = ScheduleSlot(hour: 10, profile: "day")
        let currentTime = try XCTUnwrap(calendar.date(from: DateComponents(
            year: 2026,
            month: 7,
            day: 17,
            hour: 11
        )))

        XCTAssertFalse(ApplyPolicy.requiresScheduleOverrideConfirmation(
            strategy: "schedule",
            slots: [morning, day],
            profile: "day",
            at: currentTime,
            calendar: calendar
        ))
        XCTAssertTrue(ApplyPolicy.requiresScheduleOverrideConfirmation(
            strategy: "schedule",
            slots: [morning, day],
            profile: "morning",
            at: currentTime,
            calendar: calendar
        ))
        XCTAssertFalse(ApplyPolicy.requiresScheduleOverrideConfirmation(
            strategy: "dynamic",
            slots: [morning, day],
            profile: "morning",
            at: currentTime,
            calendar: calendar
        ))
    }
}

private final class LiveWallpaperMenuStateSource {
    @Published var config: LiveConfig?
    @Published var isWorking = false
    @Published var constraints = PlaybackConstraints()
    @Published var activeVideo: URL? = URL(fileURLWithPath: "/wallpapers/day.mov")

    init(config: LiveConfig) {
        self.config = config
    }
}
