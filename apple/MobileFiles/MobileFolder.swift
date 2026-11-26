import Foundation

public struct MobileCapture: Codable, Equatable, Sendable {
    public let workspace: UUID
    public let id: UUID
    public let date, text: String

    public init(workspace: UUID, id: UUID = UUID(), date: String, text: String) {
        self.workspace = workspace
        self.id = id
        self.date = date
        self.text = text
    }
}

public struct MobileEdit: Codable, Equatable, Identifiable, Sendable {
    public let workspace, id: UUID
    public let title, revision, action: String
    public let project, status, waiting_on, until, date: String?
    public let waiting: Bool?
}

public struct MobileEditResult: Codable, Equatable, Sendable {
    public let message: String?
}

public struct MobileFolder: Sendable {
    public let root: URL
    public let id: UUID
    public var library: URL { root.appendingPathComponent("library.json") }
    public var captures: URL { root.appendingPathComponent("Captures", isDirectory: true) }

    public init(root: URL) throws {
        self.root = root
        let bytes = try Self.read(root.appendingPathComponent("hq-connection.txt"))
        guard let id = UUID(uuidString: String(decoding: bytes, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines))
        else { throw CocoaError(.fileReadCorruptFile) }
        self.id = id
    }

    public static func exists(_ url: URL) -> Bool {
        FileManager.default.fileExists(atPath: url.path) || FileManager.default.fileExists(atPath:
            url.deletingLastPathComponent().appendingPathComponent("." + url.lastPathComponent + ".icloud").path)
    }

    private static func check(_ url: URL) throws {
        if !FileManager.default.fileExists(atPath: url.path), exists(url) {
            try FileManager.default.startDownloadingUbiquitousItem(at: url)
            throw CocoaError(.fileReadNoSuchFile)
        }
        let values = try url.resourceValues(forKeys: [.isSymbolicLinkKey, .ubiquitousItemDownloadingStatusKey, .ubiquitousItemHasUnresolvedConflictsKey])
        guard values.isSymbolicLink != true, values.ubiquitousItemHasUnresolvedConflicts != true,
              (NSFileVersion.unresolvedConflictVersionsOfItem(at: url) ?? []).isEmpty else { throw CocoaError(.fileReadUnknown) }
        if values.ubiquitousItemDownloadingStatus == .notDownloaded {
            try FileManager.default.startDownloadingUbiquitousItem(at: url)
            throw CocoaError(.fileReadNoSuchFile)
        }
    }

    public static func read(_ url: URL) throws -> Data {
        try check(url)
        var error: NSError?
        var result: Result<Data, Error>?
        NSFileCoordinator().coordinate(readingItemAt: url, options: [], error: &error) { file in
            result = Result { try check(file); return try Data(contentsOf: file) }
        }
        if let error { throw error }
        guard let result else { throw CocoaError(.fileReadUnknown) }
        return try result.get()
    }

    private func write(_ url: URL, options: NSFileCoordinator.WritingOptions = [], _ body: (URL) throws -> Void) throws {
        guard try MobileFolder(root: root).id == id else { throw CocoaError(.fileWriteInvalidFileName) }
        var error: NSError?
        var result: Result<Void, Error>?
        NSFileCoordinator().coordinate(writingItemAt: url, options: options, error: &error) { target in
            result = Result {
                if Self.exists(target) { try Self.check(target) }
                try body(target)
            }
        }
        if let error { throw error }
        guard let result else { throw CocoaError(.fileWriteUnknown) }
        try result.get()
    }

    public func publish(_ data: Data) throws {
        try write(library) { target in
            if Self.exists(target), try Data(contentsOf: target) == data { return }
            try data.write(to: target, options: .atomic)
        }
    }

    public func enqueue(_ capture: MobileCapture) throws {
        guard capture.workspace == id else { throw CocoaError(.fileWriteInvalidFileName) }
        let target = captures.appendingPathComponent(capture.id.uuidString + ".json")
        try place(capture, at: target)
    }

    public func editURL(_ id: UUID) -> URL { captures.appendingPathComponent("Edit-" + id.uuidString + ".json") }
    public func resultURL(_ id: UUID) -> URL { captures.appendingPathComponent("Result-" + id.uuidString + ".json") }

    public func enqueue(_ edit: MobileEdit) throws {
        guard edit.workspace == id else { throw CocoaError(.fileWriteInvalidFileName) }
        try place(edit, at: editURL(edit.id))
    }

    public func finish(_ result: MobileEditResult, id: UUID) throws {
        try place(result, at: resultURL(id))
    }

    private func place<T: Codable & Equatable>(_ item: T, at target: URL) throws {
        try write(target) { file in
            if Self.exists(file) {
                guard try JSONDecoder().decode(T.self, from: Data(contentsOf: file)) == item
                else { throw CocoaError(.fileWriteFileExists) }
            } else {
                try JSONEncoder().encode(item).write(to: file, options: .atomic)
            }
        }
    }

    public func acknowledge(_ url: URL, bytes: Data) throws {
        try write(url, options: .forDeleting) { target in
            guard try Data(contentsOf: target) == bytes else { throw CocoaError(.fileWriteUnknown) }
            try FileManager.default.removeItem(at: target)
        }
    }
}
