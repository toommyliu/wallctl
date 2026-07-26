import AppKit
import AVFoundation
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var store: WallctlStore
    @State private var columnVisibility = NavigationSplitViewVisibility.all
    @State private var showingCreate = false
    @State private var showingSettings = false
    @State private var showingLogs = false
    @State private var showingHEIC = false
    @State private var collectionToRename: WallpaperCollection?

    var body: some View {
        NavigationSplitView(columnVisibility: $columnVisibility) {
            List(selection: $store.selection) {
                Section("Collections") {
                    ForEach(store.catalog.collections) { collection in
                        CollectionSidebarRow(collection: collection)
                            .tag(collection.name)
                            .contextMenu {
                                Button("Rename…") {
                                    collectionToRename = collection
                                }
                            }
                    }
                }
            }
            .opacity(store.hasLoadedCatalog ? 1 : 0)
            .listStyle(.sidebar)
            .navigationSplitViewColumnWidth(min: 220, ideal: 250, max: 310)
        } detail: {
            Group {
                if !store.hasLoadedCatalog {
                    ProgressView("Loading collections…")
                        .controlSize(.small)
                } else if let collection = store.selectedCollection {
                    CollectionDetail(collection: collection)
                        .transition(.opacity)
                } else {
                    ContentUnavailableView {
                        Text("No Collections")
                    } description: {
                        Text("Create a collection, then capture the wallpaper selected in System Settings.")
                    } actions: {
                        Button("New Collection…") { showingCreate = true }
                            .buttonStyle(.borderedProminent)
                            .buttonBorderShape(.capsule)
                    }
                }
            }
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    Menu {
                        Button("New Collection…") { showingCreate = true }
                        Button("Create Dynamic HEIC…") { showingHEIC = true }
                    } label: {
                        Label("Create", systemImage: "plus")
                            .labelStyle(.iconOnly)
                    }
                    .help("Create a collection or dynamic image")
                }
            }
        }
        .animation(.easeOut(duration: 0.28), value: store.hasLoadedCatalog)
        .frame(minWidth: 920, minHeight: 600)
        .sheet(isPresented: $showingCreate) { CreateCollectionSheet() }
        .sheet(isPresented: $showingHEIC) { CreateHEICSheet() }
        .sheet(isPresented: $showingSettings) { LiveSettingsView() }
        .sheet(isPresented: $showingLogs) { LogsView() }
        .sheet(item: $collectionToRename) { collection in
            RenameCollectionSheet(collection: collection)
        }
        .alert("wallctl", isPresented: Binding(
            get: { store.errorMessage != nil },
            set: { if !$0 { store.errorMessage = nil } }
        )) {
            Button("OK", role: .cancel) { store.errorMessage = nil }
        } message: {
            Text(store.errorMessage ?? "")
        }
        .overlay(alignment: .bottom) {
            if !showingSettings {
                NoticeToast()
                    .padding()
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .wallctlShowLiveSettings)) { _ in
            showingSettings = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .wallctlShowLogs)) { _ in
            showingLogs = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .wallctlShowHEICCreator)) { _ in
            showingHEIC = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .wallctlRefresh)) { _ in
            Task { await store.refresh() }
        }
    }
}

private struct CollectionSidebarRow: View {
    let collection: WallpaperCollection

    private var strategyLabel: String {
        switch collection.strategy {
        case "schedule": "Schedule"
        case "dynamic": "Dynamic"
        default: "Static"
        }
    }

    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(collection.title)
                    .font(.body.weight(.medium))
                    .lineLimit(1)
                Text("\(strategyLabel) · \(collection.profiles.count) \(collection.profiles.count == 1 ? "profile" : "profiles")")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 3)
        .accessibilityElement(children: .combine)
    }
}

private struct CreateHEICSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: WallctlStore
    @State private var light: URL?
    @State private var dark: URL?
    @State private var output: URL?
    @State private var force = false

    var body: some View {
        VStack(spacing: 0) {
            SheetHeader(
                title: "Create Dynamic HEIC",
                subtitle: "Combine light and dark images into one wallpaper that follows the system appearance."
            )

            Divider()

            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Images")
                        .font(.headline)
                    VStack(spacing: 12) {
                        fileRow("Light appearance", url: light) { light = chooseImage() }
                        Divider()
                        fileRow("Dark appearance", url: dark) { dark = chooseImage() }
                    }
                    .settingsSectionSurface(inset: 12)
                }

                VStack(alignment: .leading, spacing: 8) {
                    Text("Destination")
                        .font(.headline)
                    VStack(spacing: 0) {
                        fileRow("HEIC file", url: output) { output = chooseOutput() }
                        Divider()
                            .padding(.vertical, 12)
                        SettingsToggleRow(
                            title: "Replace an existing file",
                            detail: "Overwrite the selected destination instead of reporting an error.",
                            isOn: $force,
                            isInset: false,
                            verticalPadding: 2
                        )
                    }
                    .settingsSectionSurface(inset: 12)
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)

            Divider()

            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .buttonStyle(.bordered)
                    .buttonBorderShape(.capsule)
                    .keyboardShortcut(.cancelAction)
                Button("Create") {
                    guard let light, let dark, let output else { return }
                    store.createHEIC(light: light, dark: dark, output: output, force: force)
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
                .disabled(light == nil || dark == nil || output == nil)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 12)
        }
        .frame(width: 560, height: 460)
    }

    private func fileRow(_ title: String, url: URL?, choose: @escaping () -> Void) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 14) {
            Text(title)
                .font(.subheadline.weight(.medium))
                .frame(width: 125, alignment: .leading)

            Text(url?.path(percentEncoded: false) ?? "No file selected")
                .font(.caption)
                .lineLimit(1)
                .truncationMode(.middle)
                .foregroundStyle(url == nil ? .secondary : .primary)
                .frame(maxWidth: .infinity, alignment: .leading)

            Button("Choose…", action: choose)
                .buttonStyle(.bordered)
                .buttonBorderShape(.capsule)
        }
        .frame(minHeight: 28)
    }

    private func chooseImage() -> URL? {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        return panel.runModal() == .OK ? panel.url : nil
    }

    private func chooseOutput() -> URL? {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.heic]
        panel.nameFieldStringValue = "wallpaper.heic"
        return panel.runModal() == .OK ? panel.url : nil
    }
}

private struct CollectionDetail: View {
    @EnvironmentObject private var store: WallctlStore
    @State private var showingRemoveConfirmation = false
    @State private var showingRename = false
    let collection: WallpaperCollection

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                collectionHeader

                if collection.strategy == "schedule", !collection.slots.isEmpty {
                    ScheduleOverview(
                        collection: collection,
                        currentProfile: currentScheduleProfile
                    )
                }

                VStack(alignment: .leading, spacing: 14) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text(collection.profiles.count == 1 ? "Profile" : "Profiles")
                            .font(.title3.weight(.semibold))
                        Text("\(collection.profiles.count)")
                            .font(.title3.weight(.regular))
                            .foregroundStyle(.tertiary)
                    }

                    if collection.profiles.count == 1, let profile = collection.profiles.first {
                        ProfileCard(
                            collection: collection,
                            profile: profile,
                            isFeatured: true
                        )
                        .frame(maxWidth: 980)
                    } else {
                        LazyVGrid(
                            columns: [GridItem(.adaptive(minimum: 330), spacing: 18)],
                            alignment: .leading,
                            spacing: 18
                        ) {
                            ForEach(collection.profiles) { profile in
                                ProfileCard(collection: collection, profile: profile)
                            }
                        }
                    }
                }
            }
            .padding(.horizontal, 28)
            .padding(.vertical, 26)
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .sheet(isPresented: $showingRename) {
            RenameCollectionSheet(collection: collection)
        }
        .alert("Remove \(collection.title)?", isPresented: $showingRemoveConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Remove", role: .destructive) {
                store.remove(collection)
            }
        } message: {
            Text("This removes the collection’s captured profiles and managed wallpaper assets. This can’t be undone.")
        }
    }

    private var collectionHeader: some View {
        HStack(alignment: .top, spacing: 18) {
            VStack(alignment: .leading, spacing: 8) {
                Text(strategyLabel)
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 9)
                    .padding(.vertical, 5)
                    .background(.quaternary, in: Capsule())

                Text(collection.title)
                    .font(.system(size: 28, weight: .semibold))
                    .lineLimit(2)

                Text(collectionDescription)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 20)

            HStack(spacing: 10) {
                if store.status?.activeCollection == collection.name {
                    StatusBadge("Active", color: .green)
                } else {
                    Button("Activate") { store.activate(collection) }
                        .buttonStyle(.borderedProminent)
                        .buttonBorderShape(.capsule)
                        .disabled(!canActivate)
                        .help(canActivate ? "Make this the active collection" : "Capture or repair every profile before activating")
                }

                Menu {
                    Button("Rename Collection…") {
                        showingRename = true
                    }

                    Divider()

                    Button("Remove Collection…", role: .destructive) {
                        showingRemoveConfirmation = true
                    }
                } label: {
                    Label("Collection Actions", systemImage: "ellipsis")
                        .labelStyle(.iconOnly)
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .help("Collection actions")
            }
        }
    }

    private var currentScheduleProfile: String? {
        if store.status?.activeCollection == collection.name {
            return store.status?.expectedProfile
        }
        return ScheduleTiming.currentProfile(slots: collection.slots)
    }

    private var canActivate: Bool {
        !collection.profiles.isEmpty && collection.profiles.allSatisfy(\.valid)
    }

    private var strategyLabel: String {
        switch collection.strategy {
        case "schedule": "Schedule"
        case "dynamic": "Dynamic"
        default: "Static"
        }
    }

    private var collectionDescription: String {
        switch collection.strategy {
        case "schedule":
            "\(collection.profiles.count) wallpaper profiles rotate automatically through the day."
        case "dynamic":
            "macOS chooses the matching light or dark appearance from this profile."
        default:
            "One captured wallpaper profile is applied across your desktops."
        }
    }

}

private struct ScheduleOverview: View {
    let collection: WallpaperCollection
    let currentProfile: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Schedule")
                .font(.headline)

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 14) {
                    ForEach(collection.slots, id: \.profile) { slot in
                        let profile = collection.profiles.first { $0.name == slot.profile }
                        ScheduleSlotView(
                            hour: slot.hour,
                            title: profile?.displayName ?? slot.profile,
                            isCurrent: slot.profile == currentProfile
                        )

                        if slot.profile != collection.slots.last?.profile {
                            Divider()
                                .frame(height: 34)
                        }
                    }
                }
            }
        }
    }
}

private struct ScheduleSlotView: View {
    let hour: Int
    let title: String
    let isCurrent: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(formattedHour)
                .font(.caption.weight(.semibold))
                .foregroundStyle(isCurrent ? Color.accentColor : .secondary)
            Text(title)
                .font(.subheadline.weight(.medium))
                .lineLimit(1)
        }
        .foregroundStyle(isCurrent ? Color.primary : .secondary)
        .frame(minWidth: 110, alignment: .leading)
    }

    private var formattedHour: String {
        let date = Calendar.current.date(from: DateComponents(hour: hour)) ?? Date()
        return date.formatted(date: .omitted, time: .shortened)
    }
}

private struct StatusBadge: View {
    let title: String
    let color: Color

    init(_ title: String, color: Color) {
        self.title = title
        self.color = color
    }

    var body: some View {
        Text(title)
            .font(.caption.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(color.opacity(0.11), in: Capsule())
    }
}

private struct ProfileCard: View {
    @EnvironmentObject private var store: WallctlStore
    @State private var previewAppearance = PreviewAppearance.system
    @State private var showingApplyConfirmation = false
    let collection: WallpaperCollection
    let profile: WallpaperProfile
    var isFeatured = false

    private var hasAppearanceVariants: Bool {
        profile.lightImagePath != nil && profile.darkImagePath != nil
    }

    private var isCurrentScheduleProfile: Bool {
        guard collection.strategy == "schedule" else { return false }
        let currentProfile = store.status?.activeCollection == collection.name
            ? store.status?.expectedProfile
            : ScheduleTiming.currentProfile(slots: collection.slots)
        return currentProfile == profile.name
    }

    var body: some View {
        Group {
            if isFeatured {
                featuredLayout
            } else {
                verticalLayout(mediaHeight: 210)
            }
        }
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(.separator.opacity(0.55), lineWidth: 1)
        }
        .alert(applyConfirmationTitle, isPresented: $showingApplyConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Apply Anyway") {
                store.apply(collection, profile: profile)
            }
        } message: {
            Text(applyConfirmationMessage)
        }
    }

    private var featuredLayout: some View {
        ZStack(alignment: .bottom) {
            profileMedia
                .frame(height: 420)

            featuredFooter
                .background(.regularMaterial)
                .overlay(alignment: .top) {
                    Divider()
                }
        }
        .frame(height: 420)
    }

    private func verticalLayout(mediaHeight: CGFloat) -> some View {
        VStack(spacing: 0) {
            profileMedia
                .frame(height: mediaHeight)

            Divider()

            profileDetails
        }
    }

    private var profileMedia: some View {
        ProfileMedia(profile: profile, appearance: previewAppearance)
            .clipped()
    }

    private var profileDetails: some View {
        VStack(alignment: .leading, spacing: 14) {
            profileIdentity
            profileBadges

            if hasAppearanceVariants {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Preview")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)

                    PreviewAppearanceControl(selection: $previewAppearance)
                }
            }

            profileIssues

            Spacer(minLength: 0)

            profileActions
        }
        .padding(16)
    }

    private var featuredFooter: some View {
        VStack(alignment: .leading, spacing: 12) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 14) {
                    profileIdentity
                    profileBadges

                    if hasAppearanceVariants {
                        PreviewAppearanceControl(selection: $previewAppearance)
                    }

                    Spacer(minLength: 12)
                    profileActions
                        .fixedSize()
                }

                VStack(alignment: .leading, spacing: 12) {
                    HStack(alignment: .top, spacing: 12) {
                        profileIdentity
                        Spacer()
                        profileBadges
                    }

                    if hasAppearanceVariants {
                        PreviewAppearanceControl(selection: $previewAppearance)
                    }

                    profileActions
                }
            }

            profileIssues
        }
        .padding(16)
    }

    private var profileIdentity: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(profile.displayName ?? profile.name)
                .font(.headline)
                .lineLimit(2)
            Text(profileSubtitle)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }

    @ViewBuilder
    private var profileBadges: some View {
        if isCurrentScheduleProfile || profile.liveCapable || !profile.valid {
            HStack(spacing: 7) {
                if isCurrentScheduleProfile {
                    StatusBadge("Current", color: .green)
                }
                if profile.liveCapable {
                    StatusBadge("Live", color: .blue)
                }
                if !profile.valid {
                    StatusBadge("Needs attention", color: .orange)
                }
            }
        }
    }

    @ViewBuilder
    private var profileIssues: some View {
        if !profile.issues.isEmpty {
            Text(profile.issues.joined(separator: "\n"))
                .font(.caption)
                .foregroundStyle(.orange)
                .lineLimit(3)
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
        }
    }

    private var profileActions: some View {
        HStack(spacing: 10) {
            Button(profile.captured ? "Recapture" : "Capture") {
                store.capture(collection, profile: profile)
            }
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
            .help(profile.captured ? "Recapture the wallpaper selected in System Settings" : "Capture the wallpaper selected in System Settings")

            Spacer(minLength: 4)

            Menu {
                if let profilePath = profile.profilePath {
                    Button("Reveal Profile in Finder") {
                        NSWorkspace.shared.activateFileViewerSelecting([
                            URL(fileURLWithPath: profilePath),
                        ])
                    }

                    Divider()
                }

                Button("Choose Live Video…") { chooseVideo() }
                if profile.videoPath != nil && !profile.automaticLiveAsset {
                    Button("Use Automatic Source") { store.clearVideo(collection: collection, profile: profile) }
                }
                if let videoPath = profile.videoPath {
                    Button("Reveal Video") {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: videoPath)])
                    }
                }
            } label: {
                Label("Profile actions", systemImage: "ellipsis")
                    .labelStyle(.iconOnly)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .help("Profile actions")

            Button("Apply") {
                if requiresApplyConfirmation {
                    showingApplyConfirmation = true
                } else {
                    store.apply(collection, profile: profile)
                }
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.capsule)
            .disabled(!profile.valid)
        }
        .controlSize(.regular)
    }

    private var profileSubtitle: String {
        if collection.strategy == "schedule",
           let slot = collection.slots.first(where: { $0.profile == profile.name }) {
            let date = Calendar.current.date(from: DateComponents(hour: slot.hour)) ?? Date()
            return "Starts at \(date.formatted(date: .omitted, time: .shortened)) · \(profile.name)"
        }
        return profile.name
    }

    private var requiresApplyConfirmation: Bool {
        ApplyPolicy.requiresScheduleOverrideConfirmation(
            strategy: collection.strategy,
            slots: collection.slots,
            profile: profile.name
        )
    }

    private var applyConfirmationMessage: String {
        let currentProfileName = ScheduleTiming.currentProfile(slots: collection.slots)
        let currentProfile = collection.profiles.first { $0.name == currentProfileName }
        let currentTitle = currentProfile?.displayName ?? currentProfileName ?? "another profile"
        let startHour = collection.slots.first { $0.profile == profile.name }?.hour
        let startDescription = startHour.map(formattedHour) ?? "a different time"

        if store.liveConfig?.enabled == true {
            return "Its slot starts at \(startDescription). Live Wallpaper may restore \(currentTitle) immediately. This won’t change the schedule."
        }
        if store.status?.activeCollection == collection.name {
            return "Its slot starts at \(startDescription). The scheduler will restore \(currentTitle) on its next dispatch. This won’t change the schedule."
        }
        return "Its slot starts at \(startDescription), while \(currentTitle) is current. This is a one-time override and won’t change the schedule."
    }

    private var applyConfirmationTitle: String {
        "Apply \(profile.displayName ?? profile.name) now?"
    }

    private func formattedHour(_ hour: Int) -> String {
        let date = Calendar.current.date(from: DateComponents(hour: hour)) ?? Date()
        return date.formatted(date: .omitted, time: .shortened)
    }

    private func chooseVideo() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.movie]
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            store.assignVideo(url, collection: collection, profile: profile)
        }
    }
}

private struct ProfileMedia: View {
    @Environment(\.colorScheme) private var colorScheme
    let profile: WallpaperProfile
    let appearance: PreviewAppearance

    private var selectedVideoPath: String? {
        guard profile.automaticLiveAsset else { return profile.videoPath }
        let effectiveAppearance: PreviewAppearance = switch appearance {
        case .system: colorScheme == .dark ? .dark : .light
        case .light: .light
        case .dark: .dark
        }
        switch effectiveAppearance {
        case .system: return profile.videoPath
        case .light: return profile.lightVideoPath ?? profile.videoPath
        case .dark: return profile.darkVideoPath ?? profile.videoPath
        }
    }

    var body: some View {
        if profile.liveCapable,
           let path = selectedVideoPath {
            InlineVideoPreview(url: URL(fileURLWithPath: path), profile: profile)
                .id(path)
        } else {
            DynamicImagePreview(profile: profile, appearance: appearance)
        }
    }
}

private enum PreviewAppearance: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: Self { self }

    var title: String {
        rawValue.capitalized
    }
}

private struct PreviewAppearanceControl: View {
    @Binding var selection: PreviewAppearance

    var body: some View {
        HStack(spacing: 6) {
            ForEach(PreviewAppearance.allCases) { appearance in
                CapsuleChoiceButton(
                    title: appearance.title,
                    isSelected: selection == appearance
                ) {
                    selection = appearance
                }
            }
        }
        .controlSize(.small)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Preview appearance")
    }
}

private struct CapsuleChoiceButton: View {
    let title: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Group {
            if isSelected {
                Button(title, action: action)
                    .buttonStyle(.borderedProminent)
                    .buttonBorderShape(.capsule)
            } else {
                Button(title, action: action)
                    .buttonStyle(.bordered)
                    .buttonBorderShape(.capsule)
            }
        }
    }
}

private struct DynamicImagePreview: View {
    @Environment(\.colorScheme) private var colorScheme
    let profile: WallpaperProfile
    let appearance: PreviewAppearance

    private var hasVariants: Bool {
        profile.lightImagePath != nil && profile.darkImagePath != nil
    }

    private var effectiveAppearance: PreviewAppearance {
        switch appearance {
        case .system: colorScheme == .dark ? .dark : .light
        case .light: .light
        case .dark: .dark
        }
    }

    private var selectedPath: String? {
        switch effectiveAppearance {
        case .system: nil
        case .light: profile.lightImagePath
        case .dark: profile.darkImagePath
        }
    }

    var body: some View {
        PreviewImage(profile: profile, imagePath: hasVariants ? selectedPath : nil)
    }
}

private struct InlineVideoPreview: View {
    @StateObject private var model: InlineVideoModel
    @State private var isHovering = false
    let profile: WallpaperProfile

    init(url: URL, profile: WallpaperProfile) {
        _model = StateObject(wrappedValue: InlineVideoModel(url: url))
        self.profile = profile
    }

    var body: some View {
        ZStack {
            VideoPosterImage(url: model.url, profile: profile)
            if model.hasStarted, let player = model.player {
                InlinePlayerSurface(player: player)
                    .transition(.opacity)
            }
            Button {
                model.togglePlayback()
            } label: {
                Image(systemName: model.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                    .font(.system(size: 36))
                    .symbolRenderingMode(.palette)
                    .foregroundStyle(.white, .black.opacity(0.55))
                    .shadow(radius: 4)
            }
            .buttonStyle(.plain)
            .opacity(!model.isPlaying || isHovering ? 1 : 0)
            .accessibilityLabel(model.isPlaying ? "Pause preview" : "Play preview")
        }
        .onHover { isHovering = $0 }
        .onDisappear { model.stop() }
    }
}

private struct VideoPosterImage: View {
    let url: URL
    let profile: WallpaperProfile
    @State private var poster: NSImage?
    @State private var didFinishLoading = false

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                PreviewImage(profile: profile)
                    .frame(width: geometry.size.width, height: geometry.size.height)
                    .blur(radius: poster == nil && !didFinishLoading ? 8 : 0)
                    .opacity(poster == nil ? 1 : 0)
                if let poster {
                    Image(nsImage: poster)
                        .resizable()
                        .scaledToFill()
                        .frame(width: geometry.size.width, height: geometry.size.height)
                        .clipped()
                } else if !didFinishLoading {
                    ProgressView()
                        .controlSize(.small)
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            .clipped()
        }
        .task(id: url) {
            poster = await VideoPosterCache.shared.image(for: url)
            didFinishLoading = true
        }
        .onDisappear { poster = nil }
    }
}

private struct InlinePlayerSurface: NSViewRepresentable {
    let player: AVPlayer

    func makeNSView(context: Context) -> ChromeFreePlayerView {
        ChromeFreePlayerView(player: player)
    }

    func updateNSView(_ view: ChromeFreePlayerView, context: Context) {
        view.playerLayer.player = player
    }
}

private final class ChromeFreePlayerView: NSView {
    let playerLayer: AVPlayerLayer

    init(player: AVPlayer) {
        playerLayer = AVPlayerLayer(player: player)
        super.init(frame: .zero)
        wantsLayer = true
        layer?.masksToBounds = true
        playerLayer.videoGravity = .resizeAspectFill
        layer?.addSublayer(playerLayer)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func layout() {
        super.layout()
        playerLayer.frame = bounds
    }
}

@MainActor
private final class InlineVideoModel: ObservableObject {
    let url: URL
    private(set) var player: AVQueuePlayer?
    @Published private(set) var isPlaying = false
    @Published private(set) var hasStarted = false
    private var looper: AVPlayerLooper?

    init(url: URL) {
        self.url = url
    }

    func togglePlayback() {
        if player == nil { preparePlayer() }
        hasStarted = true
        isPlaying.toggle()
        if isPlaying {
            player?.play()
        } else {
            player?.pause()
        }
    }

    func pause() {
        player?.pause()
        isPlaying = false
    }

    func stop() {
        player?.pause()
        player?.removeAllItems()
        looper?.disableLooping()
        looper = nil
        player = nil
        hasStarted = false
        isPlaying = false
    }

    private func preparePlayer() {
        let item = AVPlayerItem(url: url)
        item.preferredForwardBufferDuration = 1
        let player = AVQueuePlayer()
        player.isMuted = true
        player.volume = 0
        player.automaticallyWaitsToMinimizeStalling = false
        player.preventsDisplaySleepDuringVideoPlayback = false
        looper = AVPlayerLooper(player: player, templateItem: item)
        self.player = player
    }
}

private struct PreviewImage: View {
    let profile: WallpaperProfile
    var imagePath: String?

    init(profile: WallpaperProfile, imagePath: String? = nil) {
        self.profile = profile
        self.imagePath = imagePath
    }

    var body: some View {
        Group {
            if let path = imagePath ?? profile.stillImagePath,
               let image = NSImage(contentsOfFile: path) {
                Image(nsImage: image).resizable().scaledToFill()
            } else if let source = profile.previewImageURL, let url = URL(string: source) {
                AsyncImage(url: url) { phase in
                    if let image = phase.image { image.resizable().scaledToFill() }
                    else { placeholder }
                }
            } else {
                placeholder
            }
        }
        .clipped()
    }

    private var placeholder: some View {
        ZStack {
            LinearGradient(colors: [.indigo.opacity(0.5), .blue.opacity(0.2)], startPoint: .topLeading, endPoint: .bottomTrailing)
            Image(systemName: profile.captured ? "photo" : "camera")
                .font(.largeTitle)
                .foregroundStyle(.white.opacity(0.8))
        }
    }
}

private struct RenameCollectionSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: WallctlStore
    @FocusState private var titleIsFocused: Bool
    @State private var title: String
    let collection: WallpaperCollection

    init(collection: WallpaperCollection) {
        self.collection = collection
        _title = State(initialValue: collection.title)
    }

    var body: some View {
        VStack(spacing: 0) {
            SheetHeader(
                title: "Rename Collection",
                subtitle: "This changes the name shown in wallctl. Its profiles, schedule, and internal identifier stay unchanged."
            )

            Divider()

            VStack(alignment: .leading, spacing: 8) {
                Text("Collection name")
                    .font(.headline)
                TextField("Collection name", text: $title)
                    .textFieldStyle(.roundedBorder)
                    .focused($titleIsFocused)
                    .onSubmit(rename)
            }
            .padding(24)

            Divider()

            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .buttonStyle(.bordered)
                    .buttonBorderShape(.capsule)
                    .keyboardShortcut(.cancelAction)
                Button("Rename", action: rename)
                    .buttonStyle(.borderedProminent)
                    .buttonBorderShape(.capsule)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canRename)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 16)
        }
        .frame(width: 480)
        .onAppear { titleIsFocused = true }
    }

    private var trimmedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canRename: Bool {
        !trimmedTitle.isEmpty && trimmedTitle != collection.title
    }

    private func rename() {
        guard canRename else { return }
        store.rename(collection, title: trimmedTitle)
        dismiss()
    }
}

private struct CreateCollectionSheet: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: WallctlStore
    @State private var name = ""
    @State private var kind = CollectionKind.static
    @State private var slots = [ScheduleSlotDraft(hour: 6, profile: "morning")]
    @FocusState private var nameIsFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            SheetHeader(
                title: "New Collection",
                subtitle: "Collections keep related wallpaper profiles together and decide when they are applied."
            )

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Name")
                            .font(.headline)
                        TextField("For example, Work Day", text: $name)
                            .focused($nameIsFocused)
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        Text("Strategy")
                            .font(.headline)
                        CollectionKindControl(selection: $kind)

                        Text(strategyDescription)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }

                    if kind == .schedule {
                        VStack(alignment: .leading, spacing: 12) {
                            HStack(alignment: .firstTextBaseline) {
                                Text("Time slots")
                                    .font(.headline)
                                Spacer()
                                Button("Add Time Slot") {
                                    slots.append(ScheduleSlotDraft())
                                }
                                .buttonStyle(.bordered)
                                .buttonBorderShape(.capsule)
                            }

                            Text("Each slot starts at the selected hour and uses the named profile until the next slot.")
                                .font(.caption)
                                .foregroundStyle(.secondary)

                            VStack(spacing: 10) {
                                ForEach($slots) { $slot in
                                    HStack(spacing: 10) {
                                        Picker("Start time", selection: $slot.hour) {
                                            ForEach(0..<24, id: \.self) { hour in
                                                Text(formattedHour(hour)).tag(hour)
                                            }
                                        }
                                        .labelsHidden()
                                        .frame(width: 125)

                                        TextField("Profile name", text: $slot.profile)

                                        Button("Remove", role: .destructive) {
                                            slots.removeAll { $0.id == slot.id }
                                        }
                                        .buttonStyle(.bordered)
                                        .buttonBorderShape(.capsule)
                                    }
                                }
                            }

                            if let scheduleValidationMessage {
                                Text(scheduleValidationMessage)
                                    .font(.caption)
                                    .foregroundStyle(.orange)
                            }
                        }
                    }
                }
                .padding(24)
            }

            Divider()

            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .buttonStyle(.bordered)
                    .buttonBorderShape(.capsule)
                    .keyboardShortcut(.cancelAction)
                Button("Create") {
                    store.create(name: name, kind: kind, slots: slots)
                    dismiss()
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
                .disabled(!canCreate)
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 16)
        }
        .frame(width: 580, height: kind == .schedule ? 540 : 400)
        .animation(.easeInOut(duration: 0.2), value: kind)
        .onAppear { nameIsFocused = true }
    }

    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && (kind != .schedule || scheduleIsValid)
    }

    private var scheduleIsValid: Bool {
        guard !slots.isEmpty else { return false }
        let profilesAreNamed = slots.allSatisfy {
            !$0.profile.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
        let hoursAreUnique = Set(slots.map(\.hour)).count == slots.count
        return profilesAreNamed && hoursAreUnique
    }

    private var scheduleValidationMessage: String? {
        guard !slots.isEmpty else { return "Add at least one time slot." }
        if slots.contains(where: { $0.profile.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }) {
            return "Every time slot needs a profile name."
        }
        if Set(slots.map(\.hour)).count != slots.count {
            return "Each time slot must start at a different hour."
        }
        return nil
    }

    private var strategyDescription: String {
        switch kind {
        case .static:
            "Capture one wallpaper setup and apply it whenever you select this collection."
        case .dynamic:
            "Capture a wallpaper with light and dark variants and let macOS follow the system appearance."
        case .schedule:
            "Capture several profiles and switch between them automatically at fixed hours."
        }
    }

    private func formattedHour(_ hour: Int) -> String {
        let date = Calendar.current.date(from: DateComponents(hour: hour)) ?? Date()
        return date.formatted(date: .omitted, time: .shortened)
    }
}

private struct CollectionKindControl: View {
    @Binding var selection: CollectionKind

    var body: some View {
        HStack(spacing: 6) {
            ForEach(CollectionKind.allCases) { kind in
                CapsuleChoiceButton(
                    title: kind.rawValue.capitalized,
                    isSelected: selection == kind
                ) {
                    selection = kind
                }
            }
        }
        .controlSize(.regular)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Strategy")
    }
}

private struct LogsView: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: WallctlStore
    @State private var selectedLogName: String?

    private var selectedLog: LogFile? {
        store.logs.first { $0.name == selectedLogName } ?? store.logs.first
    }

    var body: some View {
        VStack(spacing: 18) {
            HStack {
                Text("Logs").font(.title2.bold())
                Spacer()
                Button("Done") { dismiss() }
                    .buttonStyle(.borderedProminent)
                    .buttonBorderShape(.capsule)
                    .keyboardShortcut(.defaultAction)
            }

            if store.logs.isEmpty {
                ContentUnavailableView(
                    "No Log Entries",
                    systemImage: "text.page.slash",
                    description: Text("wallctl has not written any logs yet.")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HStack {
                    Picker("Log", selection: $selectedLogName) {
                        ForEach(store.logs) { log in
                            Text(displayName(for: log)).tag(String?.some(log.name))
                        }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .frame(width: 460)
                    Spacer()
                }

                if let selectedLog {
                    LogConsole(log: selectedLog)
                        .id(selectedLog.id)
                }
            }
        }
        .padding(24)
        .frame(width: 820, height: 560)
        .task {
            await store.loadLogs()
            if selectedLogName == nil {
                selectedLogName = store.logs.first?.name
            }
        }
    }

    private func displayName(for log: LogFile) -> String {
        log.name
            .replacingOccurrences(of: "_", with: " ")
            .capitalized
    }
}

private struct LogConsole: View {
    let log: LogFile
    @State private var didCopy = false

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text(log.name)
                    .font(.caption.weight(.medium))
                Spacer()
                Text("\(log.lines.count) lines")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Button {
                    copyLog()
                } label: {
                    Text(didCopy ? "Copied" : "Copy")
                }
                .buttonStyle(.borderless)
                .foregroundStyle(didCopy ? .green : .primary)
                Button {
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: log.path)])
                } label: {
                    Text("Reveal")
                }
                .buttonStyle(.borderless)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(.bar)

            Divider()

            ScrollView(.vertical) {
                Text(log.lines.joined(separator: "\n"))
                    .font(.system(size: 11, weight: .regular, design: .monospaced))
                    .lineSpacing(3)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                    .padding(16)
            }
            .background(Color(nsColor: .textBackgroundColor))
        }
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(.separator.opacity(0.7), lineWidth: 1)
        }
    }

    private func copyLog() {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(log.lines.joined(separator: "\n"), forType: .string)
        withAnimation(.easeOut(duration: 0.15)) {
            didCopy = true
        }
        Task {
            try? await Task.sleep(for: .seconds(1.5))
            withAnimation(.easeOut(duration: 0.15)) {
                didCopy = false
            }
        }
    }
}
