import AppKit
import Foundation
import SwiftUI

@MainActor
final class WallctlStore: ObservableObject {
    @Published private(set) var catalog = Catalog(collections: [])
    @Published private(set) var status: WallctlStatus?
    @Published private(set) var liveConfig: LiveConfig?
    @Published private(set) var logs: [LogFile] = []
    @Published private(set) var hasLoadedCatalog = false
    @Published var selection: String?
    @Published var isWorking = false
    @Published var errorMessage: String?
    @Published var notice: String?

    let liveWallpaper = LiveWallpaperController()
    private let client: WallctlClient?
    private var pollTask: Task<Void, Never>?
    private var boundaryTask: Task<Void, Never>?
    private var appearanceTask: Task<Void, Never>?
    private var appearanceObserver: NSKeyValueObservation?

    init() {
        do {
            client = try WallctlClient()
        } catch {
            client = nil
            errorMessage = error.localizedDescription
        }
    }

    var selectedCollection: WallpaperCollection? {
        catalog.collections.first { $0.name == selection }
    }

    func start() {
        guard pollTask == nil,
              ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil
        else { return }
        appearanceObserver = NSApp.observe(\.effectiveAppearance, options: [.new]) {
            [weak self] _, _ in
            Task { @MainActor in self?.scheduleAppearanceReconciliation() }
        }
        pollTask = Task { [weak self] in
            await self?.refresh(showErrors: true)
            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: .seconds(5))
                } catch {
                    return
                }
                await self?.refresh(showErrors: false)
            }
        }
    }

    func stop() {
        pollTask?.cancel()
        boundaryTask?.cancel()
        appearanceTask?.cancel()
        appearanceObserver?.invalidate()
        pollTask = nil
        boundaryTask = nil
        appearanceTask = nil
        appearanceObserver = nil
    }

    func refresh(showErrors: Bool = true) async {
        guard let client else { return }
        do {
            async let catalog = client.catalog()
            async let status = client.status()
            async let liveConfig = client.liveConfig()
            let values = try await (catalog, status, liveConfig)
            let updateCatalog = {
                self.catalog = values.0
                self.status = values.1
                self.liveConfig = values.2
                if self.selection == nil || !values.0.collections.contains(where: { $0.name == self.selection }) {
                    self.selection = values.1.activeCollection ?? values.0.collections.first?.name
                }
                self.hasLoadedCatalog = true
            }
            if hasLoadedCatalog {
                updateCatalog()
            } else {
                withAnimation(.easeOut(duration: 0.28)) {
                    updateCatalog()
                }
            }
            reconcileLiveWallpaper()
            scheduleNextBoundaryRefresh()
            if showErrors { errorMessage = nil }
        } catch {
            if showErrors { errorMessage = error.localizedDescription }
        }
    }

    func activate(_ collection: WallpaperCollection) {
        perform("Activated \(collection.title)") { client in
            try await client.use(collection: collection.name)
        }
    }

    func apply(_ collection: WallpaperCollection, profile: WallpaperProfile) {
        perform("Applied \(profile.displayName ?? profile.name)") { client in
            try await client.apply(collection: collection.name, profile: profile.name)
        }
    }

    func capture(_ collection: WallpaperCollection, profile: WallpaperProfile) {
        perform("Captured \(profile.name)") { client in
            try await client.capture(collection: collection.name, profile: profile.name)
        }
    }

    func remove(_ collection: WallpaperCollection) {
        perform("Removed \(collection.title)") { client in
            try await client.remove(collection: collection.name)
        }
    }

    func rename(_ collection: WallpaperCollection, title: String) {
        perform("Renamed to \(title)", operation: { client in
            try await client.rename(collection: collection.name, title: title)
        }, onSuccess: { response in
            self.selection = response.name
        })
    }

    func create(name: String, kind: CollectionKind, slots: [ScheduleSlotDraft]) {
        perform("Created \(name)", operation: { client in
            try await client.create(name: name, kind: kind, slots: slots)
        }, onSuccess: { collectionName in
            self.selection = collectionName
        })
    }

    func createHEIC(light: URL, dark: URL, output: URL, force: Bool) {
        perform("Created \(output.lastPathComponent)") { client in
            try await client.createHEIC(light: light, dark: dark, output: output, force: force)
        }
    }

    func assignVideo(_ video: URL, collection: WallpaperCollection, profile: WallpaperProfile) {
        perform("Assigned \(video.lastPathComponent)") { client in
            try await client.setLiveAssignment(
                collection: collection.name,
                profile: profile.name,
                video: video
            )
        }
    }

    func clearVideo(collection: WallpaperCollection, profile: WallpaperProfile) {
        perform("Restored automatic live source") { client in
            try await client.clearLiveAssignment(collection: collection.name, profile: profile.name)
        }
    }

    func setLiveEnabled(_ enabled: Bool) {
        perform(enabled ? "Live wallpaper enabled" : "Live wallpaper disabled") { client in
            try await client.setLivePreferences(enabled: enabled)
        }
    }

    func setFollowActive(_ enabled: Bool) {
        perform("Live source preference updated") { client in
            try await client.setLivePreferences(followActive: enabled)
        }
    }

    func setPauseOnBattery(_ enabled: Bool) {
        perform("Battery preference updated") { client in
            try await client.setLivePreferences(pauseOnBattery: enabled)
        }
    }

    func setPinnedCollection(_ name: String?) {
        perform("Pinned collection updated") { client in
            try await client.setLivePreferences(pinnedCollection: .some(name))
        }
    }

    func loadLogs() async {
        guard let client else { return }
        do {
            logs = try await client.logs().logs
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func stopScheduler() {
        perform("Scheduler stopped") { client in
            try await client.stopService()
        }
    }

    private func perform(_ success: String, operation: @escaping (WallctlClient) async throws -> Void) {
        perform(success, operation: operation, onSuccess: { _ in })
    }

    private func perform<Result>(
        _ success: String,
        operation: @escaping (WallctlClient) async throws -> Result,
        onSuccess: @escaping (Result) -> Void
    ) {
        guard let client else { return }
        isWorking = true
        Task {
            do {
                let result = try await operation(client)
                notice = success
                errorMessage = nil
                await refresh(showErrors: true)
                onSuccess(result)
            } catch {
                errorMessage = error.localizedDescription
            }
            isWorking = false
        }
    }

    private func reconcileLiveWallpaper(onlyIfSourceChanged: Bool = false) {
        guard let liveConfig else { return }
        let collectionName = liveConfig.followActiveCollection
            ? status?.activeCollection
            : liveConfig.pinnedCollection
        let collection = catalog.collections.first { $0.name == collectionName }
        let profileName = collectionName == status?.activeCollection
            ? status?.expectedProfile
            : pinnedProfileName(for: collection)
        let profile = collection?.profiles.first { $0.name == profileName }
        let videoURL = resolvedLiveVideoPath(for: profile).map { URL(fileURLWithPath: $0) }
        if onlyIfSourceChanged,
           LiveVideoSource.equivalent(videoURL, liveWallpaper.activeVideo) {
            return
        }
        liveWallpaper.reconcile(
            videoURL: videoURL,
            enabled: liveConfig.enabled,
            pauseOnBattery: liveConfig.pauseOnBattery
        )
    }

    private func scheduleAppearanceReconciliation() {
        appearanceTask?.cancel()
        appearanceTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .milliseconds(250))
            } catch {
                return
            }
            guard let self else { return }
            self.appearanceTask = nil
            self.reconcileLiveWallpaper(onlyIfSourceChanged: true)
        }
    }

    private func resolvedLiveVideoPath(for profile: WallpaperProfile?) -> String? {
        guard let profile else { return nil }
        let isDark = NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        return LiveVideoSource.resolvedPath(
            videoPath: profile.videoPath,
            lightVideoPath: profile.lightVideoPath,
            darkVideoPath: profile.darkVideoPath,
            automatic: profile.automaticLiveAsset,
            prefersDark: isDark
        )
    }

    private func pinnedProfileName(for collection: WallpaperCollection?) -> String? {
        guard let collection else { return nil }
        guard collection.strategy == "schedule", !collection.slots.isEmpty else {
            return collection.profiles.first?.name
        }
        return ScheduleTiming.currentProfile(slots: collection.slots)
    }

    private func scheduleNextBoundaryRefresh() {
        boundaryTask?.cancel()
        boundaryTask = nil
        guard let liveConfig else { return }
        let sourceName = liveConfig.followActiveCollection
            ? status?.activeCollection
            : liveConfig.pinnedCollection
        guard let sourceName,
              let collection = catalog.collections.first(where: { $0.name == sourceName }),
              collection.strategy == "schedule",
              !collection.slots.isEmpty
        else { return }

        let now = Date()
        let next = ScheduleTiming.nextBoundary(
            hours: collection.slots.map(\.hour),
            after: now
        )
        guard let next else { return }
        let delay = max(0, next.timeIntervalSince(now) + 0.5)
        boundaryTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(delay))
            } catch {
                return
            }
            await self?.refresh(showErrors: false)
        }
    }
}
