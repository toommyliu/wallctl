import Foundation

struct APIEnvelope<Value: Decodable>: Decodable {
    let ok: Bool
    let data: Value?
    let error: APIError?
}

struct APIError: Decodable {
    let message: String
}

struct Catalog: Decodable {
    let collections: [WallpaperCollection]
}

struct WallpaperCollection: Decodable, Identifiable, Hashable {
    var id: String { name }
    let name: String
    let title: String
    let strategy: String
    let applyMode: String
    let slots: [ScheduleSlot]
    let profiles: [WallpaperProfile]

    enum CodingKeys: String, CodingKey {
        case name, title, strategy, slots, profiles
        case applyMode = "apply_mode"
    }
}

struct ScheduleSlot: Decodable, Hashable {
    let hour: Int
    let profile: String
}

enum ScheduleTiming {
    static func currentProfile(
        slots: [ScheduleSlot],
        at date: Date = Date(),
        calendar: Calendar = .current
    ) -> String? {
        guard !slots.isEmpty else { return nil }
        let hour = calendar.component(.hour, from: date)
        let ordered = slots.sorted { $0.hour < $1.hour }
        return ordered.last(where: { $0.hour <= hour })?.profile ?? ordered.last?.profile
    }

    static func nextBoundary(
        hours: [Int],
        after date: Date = Date(),
        calendar: Calendar = .current
    ) -> Date? {
        let hours = Set(hours).sorted()
        guard let first = hours.first else { return nil }
        let today = hours.compactMap { hour -> Date? in
            var components = calendar.dateComponents([.year, .month, .day], from: date)
            components.hour = hour
            components.minute = 0
            components.second = 0
            return calendar.date(from: components)
        }
        if let next = today.first(where: { $0 > date }) {
            return next
        }
        guard let tomorrow = calendar.date(byAdding: .day, value: 1, to: date) else {
            return nil
        }
        var components = calendar.dateComponents([.year, .month, .day], from: tomorrow)
        components.hour = first
        components.minute = 0
        components.second = 0
        return calendar.date(from: components)
    }
}

enum ApplyPolicy {
    static func requiresScheduleOverrideConfirmation(
        strategy: String,
        slots: [ScheduleSlot],
        profile: String,
        at date: Date = Date(),
        calendar: Calendar = .current
    ) -> Bool {
        guard strategy == "schedule",
              let currentProfile = ScheduleTiming.currentProfile(
                  slots: slots,
                  at: date,
                  calendar: calendar
              )
        else {
            return false
        }
        return currentProfile != profile
    }
}

struct WallpaperProfile: Decodable, Identifiable, Hashable {
    var id: String { name }
    let name: String
    let captured: Bool
    let valid: Bool
    let profilePath: String?
    let provider: String?
    let aerialAssetID: String?
    let displayName: String?
    let previewImageURL: String?
    let stillImagePath: String?
    let lightImagePath: String?
    let darkImagePath: String?
    let lightVideoPath: String?
    let darkVideoPath: String?
    let videoPath: String?
    let liveCapable: Bool
    let automaticLiveAsset: Bool
    let issues: [String]

    enum CodingKeys: String, CodingKey {
        case name, captured, valid, provider, issues
        case profilePath = "profile_path"
        case aerialAssetID = "aerial_asset_id"
        case displayName = "display_name"
        case previewImageURL = "preview_image_url"
        case stillImagePath = "still_image_path"
        case lightImagePath = "light_image_path"
        case darkImagePath = "dark_image_path"
        case lightVideoPath = "light_video_path"
        case darkVideoPath = "dark_video_path"
        case videoPath = "video_path"
        case liveCapable = "live_capable"
        case automaticLiveAsset = "automatic_live_asset"
    }
}

struct WallctlStatus: Decodable {
    let activeCollection: String?
    let expectedProfile: String?
    let lastAppliedProfile: String?
    let lastAppliedAt: String?
    let liveMatchesProfile: Bool?
    let issues: [String]

    enum CodingKeys: String, CodingKey {
        case issues
        case activeCollection = "active_collection"
        case expectedProfile = "expected_profile"
        case lastAppliedProfile = "last_applied_profile"
        case lastAppliedAt = "last_applied_at"
        case liveMatchesProfile = "live_matches_profile"
    }
}

struct LiveConfig: Decodable {
    let enabled: Bool
    let followActiveCollection: Bool
    let pinnedCollection: String?
    let pauseOnBattery: Bool

    enum CodingKeys: String, CodingKey {
        case enabled
        case followActiveCollection = "follow_active_collection"
        case pinnedCollection = "pinned_collection"
        case pauseOnBattery = "pause_on_battery"
    }
}

struct LogsResponse: Decodable {
    let logs: [LogFile]
}

struct LogFile: Decodable, Identifiable {
    var id: String { name }
    let name: String
    let path: String
    let lines: [String]
}

struct EmptyResponse: Decodable {}

struct CreatedCollectionResponse: Decodable {
    let name: String
}

struct RenamedCollectionResponse: Decodable {
    let name: String
    let title: String
}

enum CollectionKind: String, CaseIterable, Identifiable {
    case `static`
    case dynamic
    case schedule

    var id: String { rawValue }
}

struct ScheduleSlotDraft: Identifiable, Equatable {
    let id = UUID()
    var hour = 6
    var profile = "morning"
}
