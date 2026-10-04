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

#if WALLCTL_LIVE
        let host = NSHostingController(rootView: AnyView(LiveWallpaperView().environmentObject(store)))
        host.sizingOptions = .preferredContentSize
        let defaultSize = host.view.fittingSize
        let styleMask: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .fullSizeContentView]
        let autosaveName: String? = nil
#else
        let host = NSHostingController(rootView: AnyView(ContentView().environmentObject(store)))
        let defaultSize = NSSize(width: 900, height: 600)
        let styleMask: NSWindow.StyleMask = [
            .titled, .closable, .miniaturizable, .resizable, .fullSizeContentView,
        ]
        let autosaveName: String? = "wallctl.window"
#endif
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: defaultSize),
            styleMask: styleMask,
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
        if let autosaveName {
            window.setFrameAutosaveName(autosaveName)
        }
        if !(autosaveName.map(window.setFrameUsingName) ?? false) {
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
