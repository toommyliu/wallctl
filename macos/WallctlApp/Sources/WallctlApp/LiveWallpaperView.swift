import AppKit
import SwiftUI

struct LiveWallpaperView: View {
    @EnvironmentObject private var store: WallctlStore

    var body: some View {
        LiveSettingsView(
            showsCloseButton: false,
            showsSchedulerControls: false,
            clearsNotices: false
        )
        .alert("wallctl", isPresented: Binding(
            get: { store.errorMessage != nil },
            set: { if !$0 { store.errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) { store.errorMessage = nil }
        } message: {
            Text(store.errorMessage ?? "")
        }
        .overlay(alignment: .bottom) {
            NoticeToast()
                .padding()
        }
    }
}

struct LiveSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: WallctlStore
    @State private var showingStopConfirmation = false
    var showsCloseButton = true
    var showsSchedulerControls = true
    var clearsNotices = true

    var body: some View {
        VStack(spacing: 0) {
            SheetHeader(
                title: "Live Wallpaper",
                subtitle: "Control desktop animation, choose its source, and see why playback is running or paused."
            )

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 10) {
                        HStack(spacing: 9) {
                            Circle()
                                .fill(playbackColor)
                                .frame(width: 8, height: 8)
                            Text(playbackTitle)
                                .font(.headline)
                        }

                        Text(playbackDetail)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)

                        Divider()

                        HStack(spacing: 8) {
                            Text("Source")
                            Spacer()
                            Text(store.liveWallpaper.activeVideo?.lastPathComponent ?? "No live-capable profile")
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)

                            if let source = store.liveWallpaper.activeVideo {
                                Button {
                                    NSWorkspace.shared.activateFileViewerSelecting([source])
                                } label: {
                                    Image(systemName: "folder")
                                }
                                .buttonStyle(.borderless)
                                .accessibilityLabel("Reveal source in Finder")
                                .help("Reveal source in Finder")
                            }
                        }
                    }
                    .settingsSectionSurface()

                    VStack(alignment: .leading, spacing: 10) {
                        Text("Playback")
                            .font(.headline)

                        VStack(spacing: 0) {
                            SettingsToggleRow(
                                title: "Animate the desktop",
                                detail: "Play a compatible video behind desktop icons.",
                                isOn: Binding(
                                    get: { store.liveConfig?.enabled ?? false },
                                    set: { store.setLiveEnabled($0) }
                                ),
                                isInset: false
                            )

                            Divider()

                            SettingsToggleRow(
                                title: "Pause on battery",
                                detail: "Resume automatically when the Mac is connected to power.",
                                isOn: Binding(
                                    get: { store.liveConfig?.pauseOnBattery ?? true },
                                    set: { store.setPauseOnBattery($0) }
                                ),
                                isInset: false
                            )
                        }
                    }
                    .settingsSectionSurface()

                    VStack(alignment: .leading, spacing: 10) {
                        Text("Source")
                            .font(.headline)

                        VStack(spacing: 0) {
                            SettingsToggleRow(
                                title: "Follow the active collection",
                                detail: "Change the live video whenever the active wallctl profile changes.",
                                isOn: Binding(
                                    get: { store.liveConfig?.followActiveCollection ?? true },
                                    set: { store.setFollowActive($0) }
                                ),
                                isInset: false
                            )

                            if store.liveConfig?.followActiveCollection == false {
                                Divider()

                                HStack {
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text("Pinned collection")
                                            .font(.subheadline.weight(.medium))
                                        Text("Keep using this collection even when another collection is active.")
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer(minLength: 20)
                                    Picker("Pinned collection", selection: Binding(
                                        get: { store.liveConfig?.pinnedCollection },
                                        set: { store.setPinnedCollection($0) }
                                    )) {
                                        Text("None").tag(String?.none)
                                        ForEach(store.catalog.collections) { collection in
                                            Text(collection.title).tag(String?.some(collection.name))
                                        }
                                    }
                                    .labelsHidden()
                                    .frame(width: 180)
                                }
                                .padding(.vertical, 12)
                            }
                        }
                    }
                    .settingsSectionSurface()

                    if showsSchedulerControls {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Scheduler")
                                .font(.headline)
                            Text("Stopping the scheduler removes its per-user LaunchAgent and clears scheduled active state. Your collections stay intact.")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                            Button("Stop Scheduler…", role: .destructive) {
                                showingStopConfirmation = true
                            }
                            .buttonStyle(.bordered)
                            .buttonBorderShape(.capsule)
                        }
                        .settingsSectionSurface()
                    }
                }
                .padding(24)
            }

            if showsCloseButton {
                Divider()

                HStack {
                    Spacer()
                    Button("Done") { dismiss() }
                        .buttonStyle(.borderedProminent)
                        .buttonBorderShape(.capsule)
                        .keyboardShortcut(.defaultAction)
                }
                .padding(.horizontal, 24)
                .padding(.vertical, 16)
            }
        }
        .frame(width: 620, height: 640)
        .onAppear {
            if clearsNotices { store.notice = nil }
        }
        .onChange(of: store.notice) {
            if clearsNotices, store.notice != nil { store.notice = nil }
        }
        .alert("Stop Scheduler?", isPresented: $showingStopConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Stop Scheduler", role: .destructive) {
                store.stopScheduler()
            }
        } message: {
            Text("Scheduled profile changes will stop until you activate a scheduled collection again.")
        }
    }

    private var playbackTitle: String {
        guard let config = store.liveConfig else { return "Loading playback status…" }
        guard config.enabled else { return "Live wallpaper is off" }
        guard store.liveWallpaper.activeVideo != nil else { return "Waiting for a live-capable profile" }
        return store.liveWallpaper.constraints.shouldPlay ? "Playing" : "Paused"
    }

    private var playbackDetail: String {
        if store.liveConfig?.enabled == false {
            return "Turn on desktop animation below to start playback."
        }
        if let reason = store.liveWallpaper.constraints.pauseReason {
            return reason
        }
        if store.liveWallpaper.activeVideo == nil {
            return "Assign a video to a profile or activate a compatible Apple Aerial wallpaper."
        }
        return "The desktop animation is visible whenever your desktops are uncovered."
    }

    private var playbackColor: Color {
        guard store.liveConfig?.enabled == true else { return .secondary }
        guard store.liveWallpaper.activeVideo != nil else { return .orange }
        return store.liveWallpaper.constraints.shouldPlay ? .green : .orange
    }
}
