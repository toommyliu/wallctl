import AppKit
import Combine
import Foundation
import SwiftUI

extension Notification.Name {
    static let wallctlShowMainWindow = Notification.Name("wallctl.show-main-window")
    static let wallctlShowLiveSettings = Notification.Name("wallctl.show-live-settings")
    static let wallctlShowLogs = Notification.Name("wallctl.show-logs")
    static let wallctlShowHEICCreator = Notification.Name("wallctl.show-heic-creator")
    static let wallctlRefresh = Notification.Name("wallctl.refresh")
}

@main
struct WallctlDesktopApp: App {
    @NSApplicationDelegateAdaptor(WallctlAppDelegate.self) private var appDelegate

    var body: some Scene {
        Settings {
            EmptyView()
        }
        .commands {
#if WALLCTL_LIVE
            CommandGroup(replacing: .appSettings) {
                Button("Live Wallpaper Settings…") {
                    NotificationCenter.default.post(name: .wallctlShowMainWindow, object: nil)
                }
                .keyboardShortcut(",", modifiers: .command)
            }
#else
            CommandGroup(replacing: .appSettings) {
                Button("Live Wallpaper Settings…") {
                    NotificationCenter.default.post(name: .wallctlShowLiveSettings, object: nil)
                }
                .keyboardShortcut(",", modifiers: .command)
            }

            CommandMenu("Tools") {
                Button("Live Wallpaper Settings…") {
                    NotificationCenter.default.post(name: .wallctlShowLiveSettings, object: nil)
                }

                Divider()

                Button("Create Dynamic HEIC…") {
                    NotificationCenter.default.post(name: .wallctlShowHEICCreator, object: nil)
                }

                Button("Refresh Collections") {
                    NotificationCenter.default.post(name: .wallctlRefresh, object: nil)
                }
                .keyboardShortcut("r", modifiers: .command)

                Divider()

                Button("Logs…") {
                    NotificationCenter.default.post(name: .wallctlShowLogs, object: nil)
                }
                .keyboardShortcut("l", modifiers: [.command, .option])
            }
#endif

            CommandGroup(replacing: .help) {}
        }
    }
}

@MainActor
final class WallctlAppDelegate: NSObject, NSApplicationDelegate {
    private let variant = WallctlAppVariant.current
    private let store = WallctlStore()
    private let mainWindow = MainWindowPresenter()
    private let liveStatusItem = LiveWallpaperStatusItem()
    private var showMainWindowObserver: NSObjectProtocol?

    func applicationDidFinishLaunching(_ notification: Notification) {
        store.start()
        showMainWindowObserver = NotificationCenter.default.addObserver(
            forName: .wallctlShowMainWindow,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.mainWindow.show(store: self.store, variant: self.variant)
            }
        }
        liveStatusItem.install(
            store: store,
            variant: variant,
            openMainWindow: { [weak self] in
                guard let self else { return }
                self.mainWindow.show(store: self.store, variant: self.variant)
            }
        )
        mainWindow.showOnLaunch(store: store, variant: variant)
        scheduleHelpMenuRemoval()
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        scheduleHelpMenuRemoval()
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let showMainWindowObserver {
            NotificationCenter.default.removeObserver(showMainWindowObserver)
        }
        store.stop()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationShouldHandleReopen(
        _ sender: NSApplication,
        hasVisibleWindows flag: Bool
    ) -> Bool {
        mainWindow.show(store: store, variant: variant)
        return true
    }

    private func scheduleHelpMenuRemoval() {
        DispatchQueue.main.async {
            guard let mainMenu = NSApp.mainMenu,
                  let helpItem = mainMenu.items.first(where: { $0.title == "Help" })
            else { return }
            mainMenu.removeItem(helpItem)
            NSApp.helpMenu = nil
        }
    }
}

@MainActor
private final class LiveWallpaperStatusItem: NSObject {
    private weak var store: WallctlStore?
    private var statusItem: NSStatusItem?
    private var subscriptions = Set<AnyCancellable>()
    private var openMainWindow: (() -> Void)?

    private let liveToggle = NSMenuItem(
        title: "Live Wallpaper",
        action: nil,
        keyEquivalent: ""
    )
    private let playbackStatus = NSMenuItem(
        title: "Status: Loading…",
        action: nil,
        keyEquivalent: ""
    )

    func install(
        store: WallctlStore,
        variant: WallctlAppVariant,
        openMainWindow: @escaping () -> Void
    ) {
        guard statusItem == nil else { return }
        self.store = store
        self.openMainWindow = openMainWindow

        let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        statusItem.button?.image = statusImage(enabled: false)
        statusItem.button?.toolTip = "\(variant.displayName) Live Wallpaper"
        statusItem.menu = makeMenu(variant: variant)
        self.statusItem = statusItem

        LiveWallpaperMenuState.updates(
            store.$liveConfig,
            store.$isWorking,
            store.liveWallpaper.$constraints,
            store.liveWallpaper.$activeVideo
        )
            .sink { [weak self] state in self?.update(state) }
            .store(in: &subscriptions)
    }

    private func makeMenu(variant: WallctlAppVariant) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.minimumWidth = LiveWallpaperMenuLayout.minimumWidth

        liveToggle.target = self
        liveToggle.action = #selector(toggleLiveWallpaper)
        LiveWallpaperMenuLayout.configureStateImages(for: liveToggle)
        menu.addItem(liveToggle)

        playbackStatus.isEnabled = false
        menu.addItem(playbackStatus)

        menu.addItem(.separator())

        let openItem = NSMenuItem(
            title: "Open \(variant.displayName)",
            action: #selector(openApp),
            keyEquivalent: ""
        )
        openItem.target = self
        menu.addItem(openItem)

        let quitItem = NSMenuItem(
            title: "Quit \(variant.displayName)",
            action: #selector(quitApp),
            keyEquivalent: "q"
        )
        quitItem.target = self
        menu.addItem(quitItem)
        return menu
    }

    private func update(_ state: LiveWallpaperMenuState) {
        liveToggle.isEnabled = state.isToggleEnabled
        liveToggle.state = state.isLiveEnabled ? .on : .off
        statusItem?.button?.image = statusImage(enabled: state.isLiveEnabled)
        setStatus(state.status)
    }

    private func setStatus(_ status: String) {
        liveToggle.title = "Live Wallpaper"
        playbackStatus.title = "Status: \(status)"
        playbackStatus.isEnabled = false
    }

    private func statusImage(enabled: Bool) -> NSImage? {
        let symbol = enabled ? "play.circle.fill" : "play.circle"
        let image = NSImage(
            systemSymbolName: symbol,
            accessibilityDescription: "Live Wallpaper"
        )
        image?.isTemplate = true
        return image
    }

    @objc private func toggleLiveWallpaper() {
        guard let store,
              let config = store.liveConfig,
              !store.isWorking
        else { return }
        store.setLiveEnabled(!config.enabled)
    }

    @objc private func openApp() {
        openMainWindow?()
    }

    @objc private func quitApp() {
        NSApplication.shared.terminate(nil)
    }
}

enum LiveWallpaperMenuLayout {
    static let minimumWidth: CGFloat = 300

    static func configureStateImages(for item: NSMenuItem) {
        guard let checkmark = NSImage(
            systemSymbolName: "checkmark",
            accessibilityDescription: nil
        ) else { return }
        checkmark.isTemplate = true

        let placeholder = NSImage(size: checkmark.size, flipped: false) { _ in true }
        placeholder.isTemplate = true
        item.onStateImage = checkmark
        item.offStateImage = placeholder
    }
}

struct LiveWallpaperMenuState: Equatable {
    let isLiveEnabled: Bool
    let isToggleEnabled: Bool
    let status: String

    static func updates(
        _ config: Published<LiveConfig?>.Publisher,
        _ isWorking: Published<Bool>.Publisher,
        _ constraints: Published<PlaybackConstraints>.Publisher,
        _ activeVideo: Published<URL?>.Publisher
    ) -> AnyPublisher<Self, Never> {
        Publishers.CombineLatest4(config, isWorking, constraints, activeVideo)
            .map(Self.init)
            .eraseToAnyPublisher()
    }

    init(
        config: LiveConfig?,
        isWorking: Bool,
        constraints: PlaybackConstraints,
        activeVideo: URL?
    ) {
        guard let config else {
            isLiveEnabled = false
            isToggleEnabled = false
            status = "Loading…"
            return
        }

        isLiveEnabled = config.enabled
        isToggleEnabled = !isWorking
        if !config.enabled {
            status = "Off"
        } else if activeVideo == nil {
            status = "Waiting for a live-capable profile"
        } else if constraints.shouldPlay {
            status = "Playing"
        } else {
            status = constraints.pauseReason ?? "Paused"
        }
    }
}
