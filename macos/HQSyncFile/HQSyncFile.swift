import Foundation
import CryptoKit

struct FileRequest: Decodable {
    let operation: String
    let path: String
    var text: String?
    var revision: String?
}

struct FileSnapshot: Codable {
    var state: String
    var text: String?
    var revision: String?
    var ubiquitous: Bool?
    var uploaded: Bool?
}

enum SyncFileError: Error { case invalidRequest, stale, unavailable, conflict }

/// Developer transport for an explicitly selected Markdown document.
/// Missing files remain unavailable; deletion requires a separate record.
struct CoordinatedDocument {
    static func metadata(_ url: URL) throws -> URLResourceValues {
        let values = try url.resourceValues(forKeys: [
            .isRegularFileKey, .isUbiquitousItemKey, .ubiquitousItemDownloadingStatusKey,
            .ubiquitousItemHasUnresolvedConflictsKey, .ubiquitousItemIsUploadedKey,
            .contentModificationDateKey,
        ])
        guard values.isRegularFile == true else { throw SyncFileError.invalidRequest }
        guard values.ubiquitousItemHasUnresolvedConflicts != true,
              (NSFileVersion.unresolvedConflictVersionsOfItem(at: url) ?? []).isEmpty
        else { throw SyncFileError.conflict }
        if values.isUbiquitousItem == true && values.ubiquitousItemDownloadingStatus != .current {
            try FileManager.default.startDownloadingUbiquitousItem(at: url)
            throw SyncFileError.unavailable
        }
        return values
    }

    static func inspect(_ url: URL) throws -> FileSnapshot {
        let values = try metadata(url)
        let data = try Data(contentsOf: url)
        guard let text = String(data: data, encoding: .utf8) else { throw SyncFileError.invalidRequest }
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        let revision = "\(digest):\(values.contentModificationDate?.timeIntervalSinceReferenceDate ?? 0)"
        return FileSnapshot(state: "available", text: text, revision: revision,
                            ubiquitous: values.isUbiquitousItem, uploaded: values.ubiquitousItemIsUploaded)
    }

    static func perform(_ request: FileRequest) throws -> FileSnapshot {
        let url = URL(fileURLWithPath: request.path).standardizedFileURL
        guard request.path.hasPrefix("/"), url.pathExtension == "md",
              url.resolvingSymlinksInPath() == url,
              ["read", "create", "replace"].contains(request.operation)
        else { throw SyncFileError.invalidRequest }
        if request.operation != "create" {
            guard FileManager.default.fileExists(atPath: url.path) else {
                return FileSnapshot(state: "unavailable")
            }
            // Ask for materialization before entering a potentially blocking coordinator.
            do { _ = try metadata(url) }
            catch SyncFileError.unavailable { return FileSnapshot(state: "unavailable") }
            catch SyncFileError.conflict { return FileSnapshot(state: "conflict") }
        }
        var coordinationError: NSError?
        var result: Result<FileSnapshot, Error>?
        let coordinator = NSFileCoordinator()
        let access: (URL) -> Void = { coordinatedURL in
            result = Result {
                if request.operation == "create" {
                    guard let text = request.text else { throw SyncFileError.invalidRequest }
                    try Data(text.utf8).write(to: coordinatedURL, options: .withoutOverwriting)
                } else if request.operation == "replace" {
                    let current = try inspect(coordinatedURL)
                    guard let expected = request.revision, current.revision == expected else {
                        throw SyncFileError.stale
                    }
                    guard let text = request.text else { throw SyncFileError.invalidRequest }
                    try Data(text.utf8).write(to: coordinatedURL, options: .atomic)
                }
                // A successful local write does not imply an iCloud upload.
                if request.operation != "read" { return FileSnapshot(state: "written") }
                return try inspect(coordinatedURL)
            }
        }
        if request.operation == "read" {
            coordinator.coordinate(readingItemAt: url, options: [], error: &coordinationError, byAccessor: access)
        } else {
            coordinator.coordinate(writingItemAt: url, options: [], error: &coordinationError, byAccessor: access)
        }
        if let error = coordinationError { throw error }
        guard let result else { throw SyncFileError.unavailable }
        do { return try result.get() }
        catch SyncFileError.unavailable { return FileSnapshot(state: "unavailable") }
        catch SyncFileError.conflict { return FileSnapshot(state: "conflict") }
    }
}

@main
struct HQSyncFile {
    static func main() {
        do {
            let request = try JSONDecoder().decode(FileRequest.self, from: FileHandle.standardInput.readDataToEndOfFile())
            let snapshot = try CoordinatedDocument.perform(request)
            FileHandle.standardOutput.write(try JSONEncoder().encode(snapshot))
        } catch {
            FileHandle.standardError.write(Data("HQSyncFile: \(error)\n".utf8))
            exit(1)
        }
    }
}
