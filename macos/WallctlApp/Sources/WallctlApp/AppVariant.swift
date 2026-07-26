enum WallctlAppVariant {
    case full
    case live

    static var current: Self {
#if WALLCTL_LIVE
        .live
#else
        .full
#endif
    }

    var displayName: String {
        switch self {
        case .full:
            "Wallctl"
        case .live:
            "Wallctl Live"
        }
    }

    var windowAutosaveName: String {
        switch self {
        case .full:
            "wallctl.main-window"
        case .live:
            "wallctl.live.main-window"
        }
    }
}
