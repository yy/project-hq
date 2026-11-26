import Foundation
import MobileFiles
import OSLog

enum MobileBridge {
    static func run(controller: ServerController) async {
        let log = Logger(subsystem: "net.yulab.project-hq", category: "mobile")
        while !Task.isCancelled {
            if let pair = UserDefaults.standard.dictionary(forKey: "HQMobileBridge") as? [String: String],
               let path = pair["folder"], let vault = pair["vault"], let id = pair["id"],
               await controller.isReady, await controller.hqDir == vault {
                do {
                    let folder = try MobileFolder(root: URL(fileURLWithPath: path))
                    guard folder.id == UUID(uuidString: id) else { throw CocoaError(.fileReadCorruptFile) }
                    do {
                        let data = try await controller.apiData(path: "/api/mobile/library", forVault: vault)
                        try folder.publish(data)
                    } catch { log.error("Mobile library pending: \(error.localizedDescription, privacy: .public)") }
                    let files = try FileManager.default.contentsOfDirectory(at: folder.captures, includingPropertiesForKeys: nil)
                    for file in files {
                        // File providers can enumerate undownloaded documents as .name.json.icloud.
                        var name = file.lastPathComponent
                        if name.hasPrefix("."), name.hasSuffix(".icloud") { name = String(name.dropFirst().dropLast(7)) }
                        guard name.hasSuffix(".json"), let expected = UUID(uuidString: String(name.dropLast(5))) else { continue }
                        do {
                            let url = folder.captures.appendingPathComponent(name)
                            let bytes = try MobileFolder.read(url)
                            let capture = try JSONDecoder().decode(MobileCapture.self, from: bytes)
                            guard capture.workspace == folder.id, capture.id == expected else { throw CocoaError(.fileReadCorruptFile) }
                            _ = try await controller.apiData(path: "/api/mobile/capture", body: bytes, forVault: vault)
                            try folder.acknowledge(url, bytes: bytes)
                        } catch { log.error("Mobile capture pending: \(error.localizedDescription, privacy: .public)") }
                    }
                } catch { log.error("Mobile sync pending: \(error.localizedDescription, privacy: .public)") }
            }
            do { try await Task.sleep(for: .seconds(20)) } catch { return }
        }
    }
}
