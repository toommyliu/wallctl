import AppKit
import SwiftUI

@MainActor
final class MainWindowPresenter: NSObject, ObservableObject, NSWindowDelegate {
    private var window: NSWindow?
    private var didShowInitialWindow = false

    func showOnLaunch(store: WallctlStore, variant: WallctlAppVariant) {
        guard !didShowInitialWindow else { return }
        didShowInitialWindow = true
        show(store: store, variant: variant)
    }

    func show(store: WallctlStore, variant: WallctlAppVariant) {
        if let window {
            NSApplication.shared.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }

        let rootView: AnyView
        let defaultSize: NSSize
#if WALLCTL_LIVE
        rootView = AnyView(LiveWallpaperView().environmentObject(store))
        defaultSize = NSSize(width: 620, height: 640)
#else
        rootView = AnyView(ContentView().environmentObject(store))
        defaultSize = NSSize(width: 1_100, height: 720)
#endif
        let host = NSHostingController(rootView: rootView)
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: defaultSize),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = variant.displayName
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.titlebarSeparatorStyle = .line
        window.toolbarStyle = .unified
        window.contentViewController = host
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.setFrameAutosaveName(variant.windowAutosaveName)
        if !window.setFrameUsingName(variant.windowAutosaveName) {
            window.center()
        }
        self.window = window

        NSApplication.shared.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        guard let closingWindow = notification.object as? NSWindow,
              closingWindow === window
        else { return }
        closingWindow.contentViewController = nil
        window = nil
#if !WALLCTL_LIVE
        VideoPosterCache.shared.removeAllImages()
#endif
    }
}
