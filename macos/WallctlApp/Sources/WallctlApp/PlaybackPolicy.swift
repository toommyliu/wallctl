import Foundation

enum LiveVideoSource {
    static func resolvedPath(
        videoPath: String?,
        lightVideoPath: String?,
        darkVideoPath: String?,
        automatic: Bool,
        prefersDark: Bool
    ) -> String? {
        guard automatic else { return videoPath }
        return prefersDark
            ? darkVideoPath ?? lightVideoPath ?? videoPath
            : lightVideoPath ?? darkVideoPath ?? videoPath
    }

    static func equivalent(_ lhs: URL?, _ rhs: URL?) -> Bool {
        switch (lhs, rhs) {
        case (nil, nil):
            true
        case let (lhs?, rhs?):
            lhs.standardizedFileURL.resolvingSymlinksInPath()
                == rhs.standardizedFileURL.resolvingSymlinksInPath()
        default:
            false
        }
    }
}

enum LoopTransitionPolicy {
    static func shouldBegin(
        currentTime: TimeInterval,
        duration: TimeInterval,
        leadTime: TimeInterval
    ) -> Bool {
        guard currentTime.isFinite,
              duration.isFinite,
              leadTime.isFinite,
              currentTime >= 0,
              leadTime > 0,
              duration > leadTime,
              currentTime < duration
        else { return false }
        return duration - currentTime <= leadTime
    }
}

struct PlaybackConstraints: Equatable {
    var enabled = true
    var screenUnavailable = false
    var lowPowerMode = false
    var onBattery = false
    var pauseOnBattery = true
    var desktopOccluded = false

    var shouldPlay: Bool {
        shouldKeepWallpaperVisible && !desktopOccluded
    }

    var shouldKeepWallpaperVisible: Bool {
        enabled
            && !screenUnavailable
            && !lowPowerMode
            && !(pauseOnBattery && onBattery)
    }

    var pauseReason: String? {
        if !enabled { return "Live wallpaper is off" }
        if screenUnavailable { return "Display is asleep or locked" }
        if lowPowerMode { return "Low Power Mode is on" }
        if pauseOnBattery && onBattery { return "Paused on battery" }
        if desktopOccluded { return "Desktop is fully covered" }
        return nil
    }
}
