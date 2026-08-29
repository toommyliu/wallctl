import AppKit
import AVFoundation
import Combine
import IOKit.ps

@MainActor
final class LiveWallpaperController: ObservableObject {
    @Published private(set) var constraints = PlaybackConstraints()
    @Published private(set) var activeVideo: URL?

    private var surfaces: [CGDirectDisplayID: DesktopVideoSurface] = [:]
    nonisolated(unsafe) private var powerSource: CFRunLoopSource?
    nonisolated(unsafe) private var occlusionTimer: Timer?
    private var currentSource: URL?
    private var idleUnloadTask: Task<Void, Never>?

    init() {
        installObservers()
        updatePowerState()
    }

    deinit {
        if let powerSource {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), powerSource, .defaultMode)
        }
        occlusionTimer?.invalidate()
        idleUnloadTask?.cancel()
    }

    func reconcile(videoURL: URL?, enabled: Bool, pauseOnBattery: Bool) {
        let playbackSettingsChanged = constraints.enabled != enabled
            || constraints.pauseOnBattery != pauseOnBattery
        constraints.enabled = enabled
        constraints.pauseOnBattery = pauseOnBattery

        guard enabled, let videoURL else {
            currentSource = videoURL
            activeVideo = videoURL
            tearDownSurfaces()
            return
        }

        let sourceChanged = !LiveVideoSource.equivalent(videoURL, currentSource)
        currentSource = videoURL
        activeVideo = videoURL
        let hadSurfaces = !surfaces.isEmpty
        if !hadSurfaces { rebuildSurfaces() }
        guard sourceChanged else {
            guard playbackSettingsChanged || !hadSurfaces else { return }
            updatePlayback()
            return
        }
        if hadSurfaces {
            for surface in surfaces.values {
                surface.setVideo(videoURL)
            }
        }
        updatePlayback()
    }

    private func installObservers() {
        let center = NSWorkspace.shared.notificationCenter
        center.addObserver(
            self,
            selector: #selector(screenUnavailable),
            name: NSWorkspace.screensDidSleepNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(screenAvailable),
            name: NSWorkspace.screensDidWakeNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(visibilityChanged),
            name: NSWorkspace.activeSpaceDidChangeNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(visibilityChanged),
            name: NSWorkspace.didActivateApplicationNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(screenUnavailable),
            name: NSWorkspace.willSleepNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(screenAvailable),
            name: NSWorkspace.didWakeNotification,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(powerModeChanged),
            name: .NSProcessInfoPowerStateDidChange,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(displaysChanged),
            name: NSApplication.didChangeScreenParametersNotification,
            object: nil
        )
        DistributedNotificationCenter.default().addObserver(
            self,
            selector: #selector(screenUnavailable),
            name: Notification.Name("com.apple.screenIsLocked"),
            object: nil
        )
        DistributedNotificationCenter.default().addObserver(
            self,
            selector: #selector(screenAvailable),
            name: Notification.Name("com.apple.screenIsUnlocked"),
            object: nil
        )
        occlusionTimer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.updateOcclusion() }
        }

        let pointer = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        if let source = IOPSNotificationCreateRunLoopSource({ pointer in
            guard let pointer else { return }
            let controller = Unmanaged<LiveWallpaperController>.fromOpaque(pointer).takeUnretainedValue()
            Task { @MainActor in controller.updatePowerState() }
        }, pointer)?.takeRetainedValue() {
            powerSource = source
            CFRunLoopAddSource(CFRunLoopGetMain(), source, .defaultMode)
        }
    }

    @objc private func screenUnavailable() {
        constraints.screenUnavailable = true
        updatePlayback()
    }

    @objc private func screenAvailable() {
        constraints.screenUnavailable = false
        updatePlayback()
    }

    // Foundation delivers this notification on an arbitrary queue.
    @objc nonisolated private func powerModeChanged() {
        Task { @MainActor [weak self] in
            self?.updatePowerState()
        }
    }

    @objc private func displaysChanged() {
        rebuildSurfaces()
    }

    @objc private func visibilityChanged() {
        updateOcclusion()
    }

    private func updatePowerState() {
        constraints.lowPowerMode = ProcessInfo.processInfo.isLowPowerModeEnabled
        constraints.onBattery = Self.isOnBattery()
        updatePlayback()
    }

    private static func isOnBattery() -> Bool {
        guard let snapshot = IOPSCopyPowerSourcesInfo()?.takeRetainedValue(),
              let sources = IOPSCopyPowerSourcesList(snapshot)?.takeRetainedValue() as? [CFTypeRef]
        else { return false }
        return sources.contains { source in
            guard let details = IOPSGetPowerSourceDescription(snapshot, source)?.takeUnretainedValue() as? [String: Any]
            else { return false }
            return details[kIOPSPowerSourceStateKey] as? String == kIOPSBatteryPowerValue
        }
    }

    private func rebuildSurfaces() {
        guard constraints.enabled, currentSource != nil || activeVideo != nil else { return }
        let screens = NSScreen.screens
        let liveIDs = Set(screens.compactMap(\.displayID))
        for (id, surface) in surfaces where !liveIDs.contains(id) {
            surface.close()
            surfaces.removeValue(forKey: id)
        }
        for screen in screens {
            guard let id = screen.displayID else { continue }
            if let surface = surfaces[id] {
                surface.updateFrame(for: screen)
                continue
            }
            let surface = DesktopVideoSurface(screen: screen)
            surfaces[id] = surface
            if let currentSource { surface.setVideo(currentSource) }
        }
        updateOcclusion()
        updatePlayback()
    }

    private func updateOcclusion() {
        constraints.desktopOccluded = Self.isDesktopFullyCovered()
        updatePlayback()
    }

    private static func isDesktopFullyCovered() -> Bool {
        let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
        guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]],
              let primary = NSScreen.screens.first
        else { return false }

        let ownPID = ProcessInfo.processInfo.processIdentifier
        let primaryHeight = primary.frame.maxY
        let windows: [CGRect] = windowList.compactMap { info in
            guard let ownerPID = info[kCGWindowOwnerPID as String] as? Int32,
                  ownerPID != ownPID,
                  info[kCGWindowLayer as String] as? Int == 0,
                  (info[kCGWindowAlpha as String] as? Double ?? 1) > 0.01,
                  let bounds = info[kCGWindowBounds as String] as? [String: CGFloat],
                  let x = bounds["X"],
                  let y = bounds["Y"],
                  let width = bounds["Width"],
                  let height = bounds["Height"],
                  width > 1,
                  height > 1
            else { return nil }
            return CGRect(x: x, y: y, width: width, height: height)
        }

        return NSScreen.screens.allSatisfy { screen in
            let frame = CGRect(
                x: screen.frame.minX,
                y: primaryHeight - screen.frame.maxY,
                width: screen.frame.width,
                height: screen.frame.height
            )
            let columns = 12
            let rows = 8
            var covered = 0
            for column in 0..<columns {
                for row in 0..<rows {
                    let point = CGPoint(
                        x: frame.minX + (CGFloat(column) + 0.5) * frame.width / CGFloat(columns),
                        y: frame.minY + (CGFloat(row) + 0.5) * frame.height / CGFloat(rows)
                    )
                    if windows.contains(where: { $0.contains(point) }) {
                        covered += 1
                    }
                }
            }
            return Double(covered) / Double(columns * rows) >= 0.9
        }
    }

    private func tearDownSurfaces() {
        idleUnloadTask?.cancel()
        idleUnloadTask = nil
        surfaces.values.forEach { $0.close() }
        surfaces.removeAll()
    }

    private func updatePlayback() {
        guard currentSource != nil else { return }
        if constraints.shouldPlay {
            idleUnloadTask?.cancel()
            idleUnloadTask = nil
            surfaces.values.forEach { $0.play() }
        } else if constraints.shouldKeepWallpaperVisible {
            idleUnloadTask?.cancel()
            idleUnloadTask = nil
            surfaces.values.forEach { $0.pause(keepingWindowVisible: true) }
        } else {
            surfaces.values.forEach { $0.pause(keepingWindowVisible: false) }
            scheduleIdleUnload()
        }
    }

    private func scheduleIdleUnload() {
        guard idleUnloadTask == nil, !surfaces.isEmpty else { return }
        idleUnloadTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(10))
            } catch {
                return
            }
            guard let self, !self.constraints.shouldPlay else { return }
            self.surfaces.values.forEach { $0.unload() }
            self.idleUnloadTask = nil
        }
    }
}

private extension NSScreen {
    var displayID: CGDirectDisplayID? {
        (deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value
    }
}

@MainActor
private final class DesktopVideoSurface {
    let window: NSWindow
    private let content: NSView
    private var active: PlayerSlot?
    private var incoming: PlayerSlot?
    private var activeReadyObserver: NSKeyValueObservation?
    private var transitionReadyObserver: NSKeyValueObservation?
    private var playbackRequested = false
    private var windowRequestedVisible = false
    private var currentURL: URL?
    private var transitionGeneration = 0
    private var crossfadeInProgressGeneration: Int?
    private let transitionDuration: CFTimeInterval = 1
    private let loopTransitionLeadTime: TimeInterval = 1.5

    init(screen: NSScreen) {
        content = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        content.wantsLayer = true
        content.layer?.backgroundColor = NSColor.black.cgColor

        window = NSWindow(
            contentRect: screen.frame,
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.contentView = content
        window.level = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.desktopIconWindow)) - 1)
        window.collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle, .fullScreenNone]
        window.ignoresMouseEvents = true
        window.acceptsMouseMovedEvents = false
        window.animationBehavior = .none
        window.isOpaque = true
        window.backgroundColor = .black
        window.hasShadow = false
    }

    func updateFrame(for screen: NSScreen) {
        window.setFrame(screen.frame, display: true)
    }

    func setVideo(_ url: URL) {
        currentURL = url
        guard playbackRequested else {
            releasePlayerSlots()
            return
        }
        guard let active else {
            installInitialVideo(url)
            return
        }

        transitionGeneration += 1
        let generation = transitionGeneration
        crossfadeInProgressGeneration = nil
        transitionReadyObserver?.invalidate()
        transitionReadyObserver = nil
        active.stopObservingApproachingEnd()
        incoming?.stop()
        incoming?.layer.removeFromSuperlayer()
        incoming = nil
        active.layer.removeAllAnimations()
        active.layer.opacity = 1

        let slot = PlayerSlot(url: url, frame: content.bounds)
        slot.layer.opacity = 0
        content.layer?.addSublayer(slot.layer)
        incoming = slot
        if playbackRequested { slot.play() }

        transitionReadyObserver = slot.layer.observe(\.isReadyForDisplay, options: [.new]) {
            [weak self, weak slot] layer, _ in
            guard layer.isReadyForDisplay, let slot else { return }
            Task { @MainActor in
                self?.beginCrossfade(to: slot, generation: generation)
            }
        }
        if slot.layer.isReadyForDisplay {
            beginCrossfade(to: slot, generation: generation)
        }
    }

    func play() {
        windowRequestedVisible = true
        playbackRequested = true
        if active == nil, let currentURL {
            installInitialVideo(currentURL)
        }
        active?.play()
        incoming?.play()

        if let incoming, incoming.layer.isReadyForDisplay {
            beginCrossfade(to: incoming, generation: transitionGeneration)
        }
        revealActiveWhenReady()
    }

    func pause(keepingWindowVisible: Bool) {
        windowRequestedVisible = keepingWindowVisible
        playbackRequested = false
        active?.pause()
        incoming?.pause()
        if keepingWindowVisible {
            if active == nil, let currentURL {
                installInitialVideo(currentURL)
            }
            if active?.layer.isReadyForDisplay != true {
                active?.play()
            }
            revealActiveWhenReady()
        } else {
            activeReadyObserver?.invalidate()
            activeReadyObserver = nil
            window.orderOut(nil)
        }
    }

    func stop() {
        windowRequestedVisible = false
        playbackRequested = false
        currentURL = nil
        releasePlayerSlots()
    }

    func unload() {
        guard !playbackRequested else { return }
        releasePlayerSlots()
    }

    private func releasePlayerSlots() {
        transitionGeneration += 1
        crossfadeInProgressGeneration = nil
        activeReadyObserver?.invalidate()
        transitionReadyObserver?.invalidate()
        activeReadyObserver = nil
        transitionReadyObserver = nil
        active?.stop()
        incoming?.stop()
        active?.layer.removeFromSuperlayer()
        incoming?.layer.removeFromSuperlayer()
        active = nil
        incoming = nil
    }

    func close() {
        stop()
        window.orderOut(nil)
        window.close()
    }

    private func installInitialVideo(_ url: URL) {
        let slot = PlayerSlot(url: url, frame: content.bounds)
        slot.layer.opacity = 1
        content.layer?.addSublayer(slot.layer)
        active = slot
        observeApproachingEnd(of: slot)
    }

    private func observeApproachingEnd(of slot: PlayerSlot) {
        slot.observeApproachingEnd(leadTime: loopTransitionLeadTime) {
            [weak self, weak slot] in
            guard let self,
                  let slot,
                  self.active === slot,
                  self.incoming == nil,
                  self.playbackRequested,
                  let currentURL = self.currentURL
            else { return }
            self.setVideo(currentURL)
        }
    }

    private func beginCrossfade(to slot: PlayerSlot, generation: Int) {
        guard playbackRequested,
              generation == transitionGeneration,
              crossfadeInProgressGeneration != generation,
              incoming === slot,
              let active
        else { return }

        transitionReadyObserver?.invalidate()
        transitionReadyObserver = nil
        crossfadeInProgressGeneration = generation
        revealWindowIfNeeded()
        CATransaction.begin()
        CATransaction.setAnimationDuration(transitionDuration)
        CATransaction.setAnimationTimingFunction(CAMediaTimingFunction(name: .easeInEaseOut))
        CATransaction.setCompletionBlock { [weak self] in
            Task { @MainActor in
                self?.finishCrossfade(generation: generation)
            }
        }
        active.layer.opacity = 0
        slot.layer.opacity = 1
        CATransaction.commit()
    }

    private func finishCrossfade(generation: Int) {
        guard generation == transitionGeneration,
              let old = active,
              let replacement = incoming
        else { return }
        old.stop()
        old.layer.removeFromSuperlayer()
        active = replacement
        incoming = nil
        crossfadeInProgressGeneration = nil
        replacement.layer.opacity = 1
        observeApproachingEnd(of: replacement)
        if playbackRequested {
            replacement.play()
            revealWindowIfNeeded()
        } else {
            replacement.pause()
            if windowRequestedVisible {
                revealWindowIfNeeded()
            } else {
                window.orderOut(nil)
            }
        }
    }

    private func revealActiveWhenReady() {
        guard let active else { return }
        if active.layer.isReadyForDisplay {
            if !playbackRequested {
                active.pause()
            }
            revealWindowIfNeeded()
        } else if activeReadyObserver == nil {
            activeReadyObserver = active.layer.observe(\.isReadyForDisplay, options: [.new]) {
                [weak self] layer, _ in
                guard layer.isReadyForDisplay else { return }
                Task { @MainActor in
                    guard let self, self.windowRequestedVisible else { return }
                    if !self.playbackRequested {
                        self.active?.pause()
                        self.incoming?.pause()
                    }
                    self.revealWindowIfNeeded()
                    self.activeReadyObserver?.invalidate()
                    self.activeReadyObserver = nil
                }
            }
        }
    }

    private func revealWindowIfNeeded() {
        // Reordering a visible all-Spaces window can interrupt an in-flight Space change.
        guard !window.isVisible else { return }
        window.orderFront(nil)
    }
}

@MainActor
private final class PlayerSlot {
    let player = AVQueuePlayer()
    let layer: AVPlayerLayer
    private var looper: AVPlayerLooper?
    private var approachingEndObserver: Any?
    private var approachingEndHandler: (() -> Void)?
    private var approachingEndLeadTime: TimeInterval?
    private var didNotifyApproachingEnd = false

    init(url: URL, frame: CGRect) {
        player.isMuted = true
        player.volume = 0
        player.automaticallyWaitsToMinimizeStalling = false
        player.preventsDisplaySleepDuringVideoPlayback = false
        layer = AVPlayerLayer(player: player)
        layer.frame = frame
        layer.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
        layer.videoGravity = .resizeAspectFill
        let item = AVPlayerItem(url: url)
        item.preferredForwardBufferDuration = 1
        looper = AVPlayerLooper(player: player, templateItem: item)
    }

    func play() { player.play() }
    func pause() { player.pause() }

    func observeApproachingEnd(leadTime: TimeInterval, handler: @escaping () -> Void) {
        stopObservingApproachingEnd()
        approachingEndLeadTime = leadTime
        approachingEndHandler = handler
        didNotifyApproachingEnd = false
        let interval = CMTime(seconds: 0.1, preferredTimescale: 600)
        approachingEndObserver = player.addPeriodicTimeObserver(
            forInterval: interval,
            queue: .main
        ) { [weak self] time in
            Task { @MainActor in
                self?.playbackTimeChanged(to: time)
            }
        }
    }

    func stopObservingApproachingEnd() {
        if let approachingEndObserver {
            player.removeTimeObserver(approachingEndObserver)
        }
        approachingEndObserver = nil
        approachingEndHandler = nil
        approachingEndLeadTime = nil
        didNotifyApproachingEnd = false
    }

    func stop() {
        stopObservingApproachingEnd()
        player.pause()
        player.removeAllItems()
        looper = nil
    }

    private func playbackTimeChanged(to time: CMTime) {
        guard !didNotifyApproachingEnd,
              let duration = player.currentItem?.duration.seconds,
              let leadTime = approachingEndLeadTime,
              LoopTransitionPolicy.shouldBegin(
                  currentTime: time.seconds,
                  duration: duration,
                  leadTime: leadTime
              )
        else { return }
        didNotifyApproachingEnd = true
        approachingEndHandler?()
    }
}
