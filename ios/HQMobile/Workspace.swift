import CryptoKit
import Foundation
import OSLog

struct Summary: Decodable {
    struct Section: Decodable { let text: String }
    struct Overview: Decodable {
        let what_is, situation, needs_you: Section
        let action_kind: String
        var actionTitle: String {
            switch action_kind {
            case "suggested": "Suggested next step"
            case "unclear": "Needs clarification"
            default: "What needs you"
            }
        }
    }
    let model, revision: String
    let overview: Overview
}

struct Card: Identifiable {
    let id, title: String
    var status: String
    let notes: String
    let summary: Summary?
    let stale: Bool
    var availableFrom: Date? = nil
    var next: String? = nil
    var revision: String? = nil

    var statusLabel: String {
        status == "my-plate" ? "My Plate" : status.replacingOccurrences(of: "-", with: " ").capitalized
    }
}

struct SavedReview: Decodable {
    let hq_review: Int
    let exported_at: TimeInterval
    let title, status, notes: String
    let summary: Summary

    static let location = URL.documentsDirectory.appendingPathComponent("Saved review.json")
    var date: Date { Date(timeIntervalSince1970: exported_at) }
    var card: Card {
        Card(id: "saved-review", title: title, status: status, notes: notes, summary: summary, stale: false)
    }

    static func load(from url: URL, save: Bool = false) throws -> SavedReview {
        let bytes = try Workspace.access(url) { try Workspace.data(url) }
        let review = try JSONDecoder().decode(Self.self, from: bytes)
        guard review.hq_review == 1 else { throw CocoaError(.fileReadUnsupportedScheme) }
        if save { try bytes.write(to: location, options: .atomic) }
        return review
    }
}

// The prototype folder has one projects track and a pre-existing HQ todo file.
enum Workspace {
    static func access<T>(_ root: URL, _ body: () throws -> T) rethrows -> T {
        let scoped = root.startAccessingSecurityScopedResource()
        defer { if scoped { root.stopAccessingSecurityScopedResource() } }
        return try body()
    }

    static func read<T>(_ url: URL, _ action: (URL) throws -> T) throws -> T {
        var error: NSError?
        var value: Result<T, Error>?
        NSFileCoordinator().coordinate(readingItemAt: url, options: [], error: &error) { location in
            value = Result { try action(location) }
        }
        if let error { throw error }
        guard let value else { throw CocoaError(.fileReadUnknown) }
        return try value.get()
    }

    static func data(_ url: URL) throws -> Data { try read(url) { try Data(contentsOf: $0) } }

    static func validate(_ root: URL) throws {
        guard try data(root.appendingPathComponent("hq-mobile-fixture.txt")) == Data("1\n".utf8) else {
            throw CocoaError(.fileReadCorruptFile)
        }
    }

    // Date-only deferrals end at local midnight. Timestamps carry their own offset.
    static func availableFrom(_ field: String?) -> Date? {
        guard let field else { return nil }
        if field.count == 10 {
            let day = DateFormatter()
            day.locale = Locale(identifier: "en_US_POSIX")
            day.calendar = Calendar(identifier: .gregorian)
            day.timeZone = .current
            day.dateFormat = "yyyy-MM-dd"
            return day.date(from: field)
        }
        let timestamp = ISO8601DateFormatter()
        if let date = timestamp.date(from: field) { return date }
        timestamp.formatOptions.insert(.withFractionalSeconds)
        return timestamp.date(from: field)
    }

    static func projects(_ root: URL) throws -> [Card] {
        try access(root) {
            try validate(root)
            let folder = root.appendingPathComponent("projects")
            let urls = try read(folder) {
                try FileManager.default.contentsOfDirectory(at: $0, includingPropertiesForKeys: nil, options: .skipsHiddenFiles)
            }
            return try urls.filter { $0.pathExtension == "md" }.map { url in
                let source = try data(url)
                guard let text = String(data: source, encoding: .utf8) else { throw CocoaError(.fileReadInapplicableStringEncoding) }
                let lines = text.trimmingPrefix("\u{feff}").replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
                guard lines.first == "---", let end = lines.dropFirst().firstIndex(of: "---") else { throw CocoaError(.fileReadCorruptFile) }
                var fields = [String: String]()
                for line in lines[1..<end] {
                    let pair = line.split(separator: ":", maxSplits: 1)
                    if pair.count == 2 {
                        let raw = pair[1].trimmingCharacters(in: .whitespaces)
                        fields[pair[0].trimmingCharacters(in: .whitespaces)] = (try? JSONDecoder().decode(String.self, from: Data(raw.utf8))) ?? raw
                    }
                }
                guard let title = fields["title"], let status = fields["status"] else { throw CocoaError(.fileReadCorruptFile) }
                let summary = try? JSONDecoder().decode(Summary.self, from: data(url.appendingPathExtension("overview.json")))
                let digest = SHA256.hash(data: Data((summary?.model ?? "").utf8) + Data([0]) + source)
                return Card(id: url.lastPathComponent, title: title, status: status,
                            notes: lines.dropFirst(end + 1).joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines),
                            summary: summary, stale: summary?.revision != digest.map { String(format: "%02x", $0) }.joined(),
                            availableFrom: availableFrom(fields["deferred_until"]))
            }.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        }
    }

    static func taskTitle(_ input: String) throws -> String {
        let title = input.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        let words = title.split(separator: " ")
        guard words.contains(where: { !"@&+".contains($0.first!) }),
              !words.contains(where: { word in ["p:", "due:", "t:"].contains(where: { word.hasPrefix($0) }) || word == "status:waiting" })
        else { throw CaptureError.plainText }
        return title
    }

    static func capture(_ input: String, in root: URL) throws {
        let title = try taskTitle(input)
        try access(root) {
            try validate(root)
            let url = root.appendingPathComponent("_tasks/todo.txt")
            var error: NSError?
            var result: Result<Void, Error>?
            NSFileCoordinator().coordinate(writingItemAt: url, options: .forMerging, error: &error) { location in
                result = Result {
                    guard (NSFileVersion.unresolvedConflictVersionsOfItem(at: location) ?? []).isEmpty else { throw CaptureError.conflict }
                    var text = try String(contentsOf: location, encoding: .utf8)
                    if !text.isEmpty && !text.hasSuffix("\n") { text += "\n" }
                    let date = DateFormatter()
                    date.locale = Locale(identifier: "en_US_POSIX")
                    date.dateFormat = "yyyy-MM-dd"
                    text += "\(date.string(from: Date())) \(title)\n"
                    try text.write(to: location, atomically: true, encoding: .utf8)
                }
            }
            if let error { throw error }
            guard let result else { throw CocoaError(.fileWriteUnknown) }
            try result.get()
        }
    }
}

enum LiveHQ {
    private static let refreshLock = NSLock()
    private static let localEditsLock = NSLock()
    private static let log = Logger(subsystem: "net.yulab.project-hq.mobile", category: "sync")
    enum ConnectionError: Error { case differentLibrary }
    struct Library: Decodable {
        struct Todo: Decodable, Identifiable {
            let line: Int
            let title: String
            let due, revision: String?
            var deferred_until: String?
            var waiting: Bool
            var id: Int { line }
            var plateFrom: Date? {
                guard let due = Workspace.availableFrom(due) else { return nil }
                return Calendar.current.date(byAdding: .day, value: -7, to: due)
            }
        }
        struct Routine: Decodable, Identifiable {
            let id, title, available_on, next_due: String
            let deferred_until, revision: String?
            var availableFrom: Date? {
                guard let available = Workspace.availableFrom(available_on) else { return nil }
                return max(available, Workspace.availableFrom(deferred_until) ?? .distantPast)
            }
        }
        struct Project: Decodable {
            let id, title, status, notes: String
            let next, deferred_until, revision: String?
            let summary: Summary?
            let stale: Bool
            let priority: Double
        }
        let hq_mobile: Int
        let projects: [Project]
        let todos: [Todo]
        let routines: [Routine]?
        let statuses: [String]?

        static func decode(_ data: Data) throws -> Self {
            let library = try JSONDecoder().decode(Self.self, from: data)
            guard library.hq_mobile == 1 else { throw CocoaError(.fileReadCorruptFile) }
            return library
        }

        var cards: [Card] {
            projects.sorted {
                if $0.priority != $1.priority { return $0.priority > $1.priority }
                return $0.title.localizedStandardCompare($1.title) == .orderedAscending
            }.map {
                Card(id: $0.id, title: $0.title, status: $0.status, notes: $0.notes, summary: $0.summary,
                     stale: $0.stale, availableFrom: Workspace.availableFrom($0.deferred_until), next: $0.next, revision: $0.revision)
            }
        }
    }

    static func cache(_ id: UUID) -> URL {
        URL.documentsDirectory.appendingPathComponent("HQ-\(id.uuidString).json")
    }

    static func outbox(_ id: UUID) -> URL {
        URL.applicationSupportDirectory.appendingPathComponent("Captures-\(id.uuidString)")
    }

    static func editsFolder(_ id: UUID) -> URL {
        URL.applicationSupportDirectory.appendingPathComponent("Edits-" + id.uuidString)
    }

    static func pending(_ id: UUID) throws -> [MobileEdit] {
        try localEditsLock.withLock { try readPending(id) }
    }

    private static func readPending(_ id: UUID) throws -> [MobileEdit] {
        let dir = editsFolder(id)
        guard FileManager.default.fileExists(atPath: dir.path) else { return [] }
        return try FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "json" }
            .map { try JSONDecoder().decode(MobileEdit.self, from: Data(contentsOf: $0)) }
    }

    static func save(_ edit: MobileEdit) throws {
        localEditsLock.lock()
        defer { localEditsLock.unlock() }
        if try readPending(edit.workspace).contains(where: { $0.targets(project: edit.project, routine: edit.routine, revision: edit.revision) }) {
            throw NSError(domain: "HQ", code: 1, userInfo: [NSLocalizedDescriptionKey: "An edit to this item is already waiting to sync."])
        }
        let dir = editsFolder(edit.workspace)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try JSONEncoder().encode(edit).write(to: dir.appendingPathComponent(edit.id.uuidString + ".json"), options: .atomic)
    }

    static func issuesKey(_ id: UUID) -> String { "editIssues-" + id.uuidString }

    // Each file is an independent, idempotent delivery. Keep failed items for the next pass.
    private static func drain(_ folder: URL, send: (URL) throws -> Void) {
        guard FileManager.default.fileExists(atPath: folder.path) else { return }
        do {
            let files = try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
            for file in files where file.pathExtension == "json" {
                do { try send(file) } catch { logRetry(error) }
            }
        } catch { logRetry(error) }
    }

    static func logRetry(_ error: Error) {
        let cause = error as NSError
        log.info("Sync will retry (\(cause.domain, privacy: .public) \(cause.code, privacy: .public))")
    }

    private static func deliverEdits(_ live: MobileFolder, libraryAvailable: Bool) {
        drain(editsFolder(live.id)) { file in
            let edit = try JSONDecoder().decode(MobileEdit.self, from: Data(contentsOf: file))
            let response = live.resultURL(edit.id)
            guard MobileFolder.exists(response) else {
                try live.enqueue(edit)
                return
            }
            // Keep the local edit visible against the cached lists until we can refresh them.
            guard libraryAvailable else { return }
            let bytes = try MobileFolder.read(response)
            let result = try JSONDecoder().decode(MobileEditResult.self, from: bytes)
            if let message = result.message {
                let key = issuesKey(live.id)
                var issues = UserDefaults.standard.dictionary(forKey: key) as? [String: String] ?? [:]
                issues[edit.id.uuidString] = edit.title + " — " + message
                UserDefaults.standard.set(issues, forKey: key)
            }
            try localEditsLock.withLock { try FileManager.default.removeItem(at: file) }
            try live.acknowledge(response, bytes: bytes)
        }
    }

    // An existing Files grant also covers its new HQ Live subfolder.
    static func open(_ root: URL, expected: UUID?) throws -> MobileFolder {
        let direct = root.lastPathComponent == "HQ Live" || MobileFolder.exists(root.appendingPathComponent("hq-connection.txt"))
        let live = try MobileFolder(root: direct ? root : root.appendingPathComponent("HQ Live"))
        guard expected == nil || expected == live.id else { throw ConnectionError.differentLibrary }
        return live
    }

    static func refresh(_ root: URL, expected: UUID?) throws -> (UUID, Library) {
        // Foreground refresh, pull-to-refresh and capture can overlap.
        refreshLock.lock()
        defer { refreshLock.unlock() }
        return try Workspace.access(root) {
            let live = try open(root, expected: expected)
            let snapshot = Result {
                let bytes = try MobileFolder.read(live.library)
                let library = try Library.decode(bytes)
                let target = cache(live.id)
                if (try? Data(contentsOf: target)) != bytes { try bytes.write(to: target, options: .atomic) }
                return library
            }
            drain(outbox(live.id)) { file in
                let item = try JSONDecoder().decode(MobileCapture.self, from: Data(contentsOf: file))
                try live.enqueue(item)
                try FileManager.default.removeItem(at: file)
            }
            deliverEdits(live, libraryAvailable: (try? snapshot.get()) != nil)
            return (live.id, try snapshot.get())
        }
    }

    static func capture(_ text: String, id: UUID) throws {
        let date = DateFormatter()
        date.locale = Locale(identifier: "en_US_POSIX")
        date.dateFormat = "yyyy-MM-dd"
        let item = MobileCapture(workspace: id, date: date.string(from: Date()), text: try Workspace.taskTitle(text))
        let folder = outbox(id)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try JSONEncoder().encode(item).write(to: folder.appendingPathComponent(item.id.uuidString + ".json"), options: .atomic)
    }
}

enum CaptureError: LocalizedError {
    case plainText, conflict
    var errorDescription: String? {
        switch self {
        case .plainText: "Enter a task title in plain text."
        case .conflict: "Resolve the conflicting copies of todo.txt before saving."
        }
    }
}
