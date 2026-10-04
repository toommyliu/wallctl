import AppKit
import SwiftUI

struct LiveWallpaperView: View {
    @EnvironmentObject private var store: WallctlStore

    var body: some View {
        LiveSettingsView(showsCloseButton: false, showsSchedulerControls: false)
            .alert("wallctl", isPresented: Binding(
                get: { store.errorMessage != nil },
                set: { if !$0 { store.errorMessage = nil } }
            )) {
                Button("OK", role: .cancel) { store.errorMessage = nil }
            } message: {
                Text(store.errorMessage ?? "")
            }
    }
}

struct LiveSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: WallctlStore
    @State private var showingStopConfirmation = false
    var showsCloseButton = true
    var showsSchedulerControls = true

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 12) {
                LivePlaybackHeader(
                    state: playback,
                    isOn: Binding(
                        get: { store.liveConfig?.enabled ?? false },
                        set: { store.setLiveEnabled($0) }
                    )
                )
                .settingsSectionSurface(inset: 12)

                VStack(spacing: 0) {
                    LiveSettingsRow(title: "Source") {
                        Picker("Source", selection: source) {
                            Text("Active collection").tag(LiveSource.activeCollection)
                            Divider()
                            ForEach(store.catalog.collections) { collection in
                                Text(collection.title).tag(LiveSource.collection(collection.name))
                            }
                            if case .collection(let name) = source.wrappedValue,
                               !store.catalog.collections.contains(where: { $0.name == name }) {
                                Text(name ?? "None").tag(source.wrappedValue)
                            }
                        }
                        .labelsHidden()
                        .fixedSize()
                    }

                    if let video = store.liveWallpaper.activeVideo {
                        Divider()
                        LiveSettingsRow(title: "Video") {
                            HStack(spacing: 6) {
                                Text(videoName(video))
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                Button {
                                    NSWorkspace.shared.activateFileViewerSelecting([video])
                                } label: {
                                    Image(systemName: "folder")
                                }
                                .buttonStyle(.borderless)
                                .help("Show in Finder")
                                .accessibilityLabel("Show video in Finder")
                            }
                        }
                    }

                    Divider()
                    SettingsToggleRow(
                        title: "Pause on battery",
                        detail: "Resumes when the Mac is connected to power.",
                        isOn: Binding(
                            get: { store.liveConfig?.pauseOnBattery ?? true },
                            set: { store.setPauseOnBattery($0) }
                        ),
                        isInset: false,
                        verticalPadding: 10
                    )
                }
                .padding(.horizontal, 12)
                .settingsSectionSurface(inset: 0)

                if playback.phase == .noVideo {
                    Text("Use an Aerial wallpaper, or assign a video to the current profile in Wallctl.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 12)
                }

                if showsSchedulerControls {
                    LiveSettingsRow(
                        title: "Scheduler",
                        detail: "Removes the scheduler's LaunchAgent. Collections are kept."
                    ) {
                        Button("Stop…", role: .destructive) {
                            showingStopConfirmation = true
                        }
                    }
                    .padding(.horizontal, 12)
                    .settingsSectionSurface(inset: 0)
                }
            }
            .padding(16)

            if showsCloseButton {
                Divider()

                HStack {
                    Spacer()
                    Button("Done") { dismiss() }
                        .buttonStyle(.borderedProminent)
                        .buttonBorderShape(.capsule)
                        .keyboardShortcut(.defaultAction)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
        }
        .frame(width: 400)
        .fixedSize(horizontal: false, vertical: true)
        .onAppear { store.notice = nil }
        .onChange(of: store.notice) {
            if store.notice != nil { store.notice = nil }
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

    private var playback: LivePlaybackState {
        LivePlaybackState(
            config: store.liveConfig,
            isWorking: store.isWorking,
            constraints: store.liveWallpaper.constraints,
            activeVideo: store.liveWallpaper.activeVideo
        )
    }

    private var source: Binding<LiveSource> {
        Binding(
            get: {
                guard let config = store.liveConfig, !config.followActiveCollection else {
                    return .activeCollection
                }
                return .collection(config.pinnedCollection)
            },
            set: { source in
                switch source {
                case .activeCollection:
                    store.setLiveSource(pinnedCollection: nil)
                case .collection(let name?):
                    store.setLiveSource(pinnedCollection: name)
                case .collection(nil):
                    break
                }
            }
        )
    }

    private func videoName(_ video: URL) -> String {
        if let profile = store.liveProfile,
           profile.automaticLiveAsset,
           let displayName = profile.displayName {
            return displayName
        }
        return video.deletingPathExtension().lastPathComponent
    }
}

private enum LiveSource: Hashable {
    case activeCollection
    case collection(String?)
}

private struct LiveSettingsRow<Content: View>: View {
    let title: String
    var detail: String?
    @ViewBuilder let content: Content

    var body: some View {
        HStack(spacing: 18) {
            VStack(alignment: .leading, spacing: 3) {
                Text(title)
                    .font(.subheadline.weight(.medium))
                if let detail {
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            Spacer(minLength: 16)

            content
        }
        .padding(.vertical, 10)
    }
}

private struct LivePlaybackHeader: View {
    let state: LivePlaybackState
    @Binding var isOn: Bool

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.white)
                .contentTransition(.symbolEffect(.replace))
                .frame(width: 32, height: 32)
                .background(tint.gradient, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 2) {
                Text("Live Wallpaper")
                    .font(.headline)
                Text(state.status)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer(minLength: 12)

            Toggle("Live Wallpaper", isOn: $isOn)
                .labelsHidden()
                .toggleStyle(.switch)
                .disabled(!state.isToggleEnabled)
        }
        .padding(.vertical, 2)
    }

    private var symbol: String {
        switch state.phase {
        case .loading: "ellipsis"
        case .off: "play.slash.fill"
        case .noVideo: "film"
        case .playing: "play.fill"
        case .paused: "pause.fill"
        }
    }

    private var tint: Color {
        switch state.phase {
        case .loading, .off: .gray
        case .noVideo, .paused: .orange
        case .playing: .green
        }
    }
}
