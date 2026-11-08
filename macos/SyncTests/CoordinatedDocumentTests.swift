import XCTest
@testable import HQSyncFile

final class CoordinatedDocumentTests: XCTestCase {
    func testRevisionsRejectLostUpdatesAndCreateRejectsReplacement() throws {
        let directory = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let path = directory.appendingPathComponent("project.md").path
        func request(_ operation: String, _ text: String? = nil, _ revision: String? = nil) throws -> FileSnapshot {
            try CoordinatedDocument.perform(FileRequest(operation: operation, path: path, text: text, revision: revision))
        }
        XCTAssertEqual(try request("read").state, "unavailable")
        XCTAssertEqual(try request("create", "Initial").state, "written")
        let first = try request("read")
        XCTAssertThrowsError(try request("create", "Overwrite"))
        XCTAssertThrowsError(try request("replace", "Overwrite"))
        XCTAssertEqual(try request("replace", "Newer", first.revision).state, "written")
        XCTAssertThrowsError(try request("replace", "Stale", first.revision))
        XCTAssertEqual(try request("read").text, "Newer")
    }

    func testRejectsSymlinkAndUnsupportedOperations() throws {
        let directory = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let original = directory.appendingPathComponent("original.md")
        let link = directory.appendingPathComponent("link.md")
        try Data("Preserve".utf8).write(to: original)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: original)
        XCTAssertThrowsError(try CoordinatedDocument.perform(FileRequest(operation: "read", path: link.path)))
        XCTAssertThrowsError(try CoordinatedDocument.perform(FileRequest(operation: "delete", path: original.path)))
        XCTAssertEqual(try String(contentsOf: original), "Preserve")
    }
}
