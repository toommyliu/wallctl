import SwiftUI

struct SheetHeader: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title)
                .font(.title2.weight(.semibold))
            Text(subtitle)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 24)
        .padding(.vertical, 20)
    }
}

private struct SettingsSectionSurface: ViewModifier {
    let inset: CGFloat

    func body(content: Content) -> some View {
        content
            .padding(inset)
            .background(
                Color.primary.opacity(0.045),
                in: RoundedRectangle(cornerRadius: 12, style: .continuous)
            )
    }
}

extension View {
    func settingsSectionSurface(inset: CGFloat = 16) -> some View {
        modifier(SettingsSectionSurface(inset: inset))
    }
}

struct SettingsToggleRow: View {
    let title: String
    let detail: String
    @Binding var isOn: Bool
    var isInset = true
    var verticalPadding: CGFloat = 14

    var body: some View {
        HStack(spacing: 18) {
            VStack(alignment: .leading, spacing: 3) {
                Text(title)
                    .font(.subheadline.weight(.medium))
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer(minLength: 16)

            Toggle(title, isOn: $isOn)
                .labelsHidden()
                .toggleStyle(.switch)
        }
        .padding(.horizontal, isInset ? 14 : 0)
        .padding(.vertical, verticalPadding)
    }
}

struct NoticeToast: View {
    @EnvironmentObject private var store: WallctlStore

    var body: some View {
        if let notice = store.notice {
            Text(notice)
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(.regularMaterial, in: Capsule())
                .shadow(color: .black.opacity(0.18), radius: 8, y: 3)
                .zIndex(1)
                .task(id: notice) {
                    try? await Task.sleep(for: .seconds(2))
                    if store.notice == notice { store.notice = nil }
                }
        }
    }
}
