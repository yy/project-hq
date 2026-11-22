import CryptoKit
import Foundation

struct Summary: Decodable {
    struct Section: Decodable { let text: String }
    struct Overview: Decodable {
        let what_is, situation, needs_you: Section
        let action_kind: String
    }
    let model, revision: String
    let overview: Overview
}

struct Card: Identifiable {
    let id, title, status, notes: String
    let summary: Summary?
    let stale: Bool
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
                            summary: summary, stale: summary?.revision != digest.map { String(format: "%02x", $0) }.joined())
            }.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        }
    }

    static func capture(_ input: String, in root: URL) throws {
        let title = input.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        let words = title.split(separator: " ")
        guard words.contains(where: { !"@&+".contains($0.first!) }),
              !words.contains(where: { word in ["p:", "due:", "t:"].contains(where: { word.hasPrefix($0) }) || word == "status:waiting" })
        else { throw CaptureError.plainText }
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

enum CaptureError: LocalizedError {
    case plainText, conflict
    var errorDescription: String? {
        switch self {
        case .plainText: "Enter a task title in plain text."
        case .conflict: "Resolve the conflicting copies of todo.txt before saving."
        }
    }
}
