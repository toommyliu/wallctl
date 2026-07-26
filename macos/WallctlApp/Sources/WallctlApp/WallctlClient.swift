import Foundation

enum WallctlClientError: LocalizedError {
    case binaryNotFound
    case processFailed(String)
    case invalidResponse(String)

    var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            "Could not find wallctl. Set WALLCTL_BINARY or place it beside the app executable."
        case .processFailed(let message), .invalidResponse(let message):
            message
        }
    }
}

actor WallctlClient {
    private let binaryURL: URL
    private let decoder = JSONDecoder()

    init(binaryURL: URL? = nil) throws {
        if let binaryURL {
            self.binaryURL = binaryURL
            return
        }
        guard let discovered = Self.findBinary() else {
            throw WallctlClientError.binaryNotFound
        }
        self.binaryURL = discovered
    }

    func catalog() async throws -> Catalog {
        try await call(["catalog"], as: Catalog.self)
    }

    func status() async throws -> WallctlStatus {
        try await call(["status"], as: WallctlStatus.self)
    }

    func liveConfig() async throws -> LiveConfig {
        try await call(["live", "get"], as: LiveConfig.self)
    }

    func logs(lines: Int = 100) async throws -> LogsResponse {
        try await call(["logs", "--lines", String(lines)], as: LogsResponse.self)
    }

    func use(collection: String) async throws {
        _ = try await call(["use", collection], as: EmptyResponse.self)
    }

    func apply(collection: String, profile: String) async throws {
        _ = try await call(["apply", collection, profile], as: EmptyResponse.self)
    }

    func capture(collection: String, profile: String) async throws {
        _ = try await call(["capture", collection, profile], as: EmptyResponse.self)
    }

    func remove(collection: String) async throws {
        _ = try await call(["remove", collection], as: EmptyResponse.self)
    }

    func rename(collection: String, title: String) async throws -> RenamedCollectionResponse {
        try await call(["rename", collection, title], as: RenamedCollectionResponse.self)
    }

    func create(name: String, kind: CollectionKind, slots: [ScheduleSlotDraft]) async throws -> String {
        var arguments = ["new", kind.rawValue, name]
        if kind == .schedule {
            for slot in slots {
                arguments += ["--slot", "\(slot.hour):\(slot.profile)"]
            }
        }
        return try await call(arguments, as: CreatedCollectionResponse.self).name
    }

    func createHEIC(light: URL, dark: URL, output: URL, force: Bool) async throws {
        var arguments = [
            "heic", "--light", light.path, "--dark", dark.path, "--output", output.path,
        ]
        if force { arguments.append("--force") }
        _ = try await call(arguments, as: EmptyResponse.self)
    }

    func setLiveAssignment(collection: String, profile: String, video: URL) async throws {
        _ = try await call(
            ["live", "set-assignment", collection, profile, "--video", video.path],
            as: LiveConfig.self
        )
    }

    func clearLiveAssignment(collection: String, profile: String) async throws {
        _ = try await call(
            ["live", "clear-assignment", collection, profile],
            as: LiveConfig.self
        )
    }

    func setLivePreferences(
        enabled: Bool? = nil,
        followActive: Bool? = nil,
        pinnedCollection: String?? = nil,
        pauseOnBattery: Bool? = nil
    ) async throws {
        var arguments = ["live", "set-preferences"]
        if let enabled { arguments += ["--enabled", String(enabled)] }
        if let followActive { arguments += ["--follow-active-collection", String(followActive)] }
        if let pinnedCollection {
            if let value = pinnedCollection {
                arguments += ["--pinned-collection", value]
            } else {
                arguments.append("--clear-pinned-collection")
            }
        }
        if let pauseOnBattery { arguments += ["--pause-on-battery", String(pauseOnBattery)] }
        _ = try await call(arguments, as: LiveConfig.self)
    }

    func stopService() async throws {
        _ = try await call(["service", "stop"], as: EmptyResponse.self)
    }

    private func call<Value: Decodable>(_ arguments: [String], as type: Value.Type) async throws -> Value {
        let result = try await run(["api"] + arguments)
        let envelope: APIEnvelope<Value>
        do {
            envelope = try decoder.decode(APIEnvelope<Value>.self, from: result)
        } catch {
            throw WallctlClientError.invalidResponse(
                "wallctl returned invalid JSON: \(String(data: result, encoding: .utf8) ?? "<binary data>")"
            )
        }
        guard envelope.ok, let value = envelope.data else {
            throw WallctlClientError.processFailed(envelope.error?.message ?? "wallctl failed")
        }
        return value
    }

    private func run(_ arguments: [String]) async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            let process = Process()
            let stdout = Pipe()
            let stderr = Pipe()
            process.executableURL = binaryURL
            process.arguments = arguments
            process.standardOutput = stdout
            process.standardError = stderr
            process.terminationHandler = { process in
                let output = stdout.fileHandleForReading.readDataToEndOfFile()
                let errorData = stderr.fileHandleForReading.readDataToEndOfFile()
                if process.terminationStatus == 0 {
                    continuation.resume(returning: output)
                } else {
                    let message = String(data: errorData, encoding: .utf8) ?? "wallctl exited with status \(process.terminationStatus)"
                    continuation.resume(throwing: WallctlClientError.processFailed(message))
                }
            }
            do {
                try process.run()
            } catch {
                continuation.resume(throwing: error)
            }
        }
    }

    nonisolated private static func findBinary() -> URL? {
        let fileManager = FileManager.default
        var candidates = [URL]()
        if let configured = ProcessInfo.processInfo.environment["WALLCTL_BINARY"] {
            candidates.append(URL(fileURLWithPath: configured))
        }
        candidates.append(
            Bundle.main.bundleURL.appending(path: "Contents/Helpers/wallctl")
        )
        candidates += [
            URL(fileURLWithPath: "/opt/homebrew/bin/wallctl"),
            URL(fileURLWithPath: "/usr/local/bin/wallctl"),
            URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appending(path: "target/debug/wallctl"),
            URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appending(path: "target/release/wallctl"),
            URL(fileURLWithPath: #filePath)
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .appending(path: "target/debug/wallctl"),
        ]
        return candidates.first { fileManager.isExecutableFile(atPath: $0.path) }
    }
}
