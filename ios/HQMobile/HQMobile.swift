import SwiftUI
import UniformTypeIdentifiers

@main
struct HQMobile: App {
    var body: some Scene { WindowGroup { ProjectsView() } }
}

struct ProjectsView: View {
    @Environment(\.scenePhase) private var phase
    @State private var folder: URL?
    @State private var projects: [Card] = []
    @State private var picking = false
    @State private var pickingReview = false
    @State private var review: SavedReview?
    @State private var capturing = false
    @State private var draft = ""
    @State private var saving = false
    @State private var saved = false
    @State private var problem: String?
    @State private var navigation: [String] = []
    @State private var refreshID = UUID()

    var body: some View {
        NavigationStack(path: $navigation) {
            List {
                if let review {
                    Section("Offline review · " + review.date.formatted(date: .abbreviated, time: .omitted)) {
                        NavigationLink(value: review.card.id) {
                            VStack(alignment: .leading, spacing: 5) {
                                Text(review.title).font(.headline)
                                Text(review.summary.overview.actionTitle).font(.caption).foregroundStyle(.secondary)
                                Text(review.summary.overview.needs_you.text)
                            }
                        }
                    }
                }
                if let folder {
                    Section(folder.lastPathComponent) {
                        ForEach(projects) { card in
                            NavigationLink(value: card.id) {
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(card.title).font(.headline)
                                    Text(card.status).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                        if projects.isEmpty { Text("No projects in this folder.") }
                    }
                }
                if saved { Label("Task saved to Todos", systemImage: "checkmark.circle").foregroundStyle(.secondary) }
            }
            .navigationTitle("Projects")
            .navigationDestination(for: String.self) { id in
                if let card = (review?.card.id == id ? review?.card : projects.first(where: { $0.id == id })) {
                    List {
                        Section { Text(card.title).font(.title2.bold()); Text(card.status).foregroundStyle(.secondary) }
                        if let summary = card.summary {
                            Section(summary.overview.actionTitle) {
                                Text(summary.overview.needs_you.text)
                            }
                            Section("Overview") {
                                Text(summary.overview.what_is.text)
                                Text(summary.overview.situation.text)
                                if card.stale { Label("Overview predates the latest edit", systemImage: "clock") }
                            }
                        } else { Text("No saved overview.") }
                        Section { DisclosureGroup("Full notes") { Text(card.notes).textSelection(.enabled) } }
                    }.refreshable { await refresh() }
                } else { ContentUnavailableView("Project unavailable", systemImage: "doc") }
            }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Menu("Open", systemImage: "folder") {
                        Button("Import project review") { pickingReview = true; picking = true }
                        Button("Open folder") { pickingReview = false; picking = true }
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Quick capture", systemImage: "plus") { saved = false; capturing = true }.disabled(folder == nil)
                }
            }
            .refreshable { await refresh() }
            .fileImporter(isPresented: $picking, allowedContentTypes: pickingReview ? [.json] : [.folder]) { result in
                do {
                    let url = try result.get()
                    if pickingReview { Task { await loadReview(url, save: true) } }
                    else { Task { await load(url, remember: true) } }
                }
                catch { problem = error.localizedDescription }
            }
            .sheet(isPresented: $capturing) {
                NavigationStack {
                    Form {
                        TextField("What needs doing?", text: $draft)
                        if let problem { Text(problem).foregroundStyle(.red) }
                    }
                    .navigationTitle("Quick capture")
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) { Button("Cancel") { capturing = false }.disabled(saving) }
                        ToolbarItem(placement: .confirmationAction) { Button("Save") { Task { await capture() } }.disabled(saving || draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) }
                    }
                }.interactiveDismissDisabled(saving)
            }
            .alert("HQ", isPresented: Binding(get: { problem != nil && !capturing }, set: { if !$0 { problem = nil } })) {
                Button("OK") { problem = nil }
            } message: { Text(problem ?? "") }
        }.task {
            if FileManager.default.fileExists(atPath: SavedReview.location.path) {
                await loadReview(SavedReview.location)
            }
            do {
                if let bookmark = UserDefaults.standard.data(forKey: "workspace") {
                    var stale = false
                    let root = try URL(resolvingBookmarkData: bookmark, options: [], relativeTo: nil, bookmarkDataIsStale: &stale)
                    await load(root, remember: stale)
                } else {
                    let root = URL.documentsDirectory.appendingPathComponent("Sample HQ")
                    if !FileManager.default.fileExists(atPath: root.path) {
                        guard let sample = Bundle.main.url(forResource: "SampleHQ", withExtension: nil) else { throw CocoaError(.fileNoSuchFile) }
                        try FileManager.default.copyItem(at: sample, to: root)
                    }
                    await load(root)
                }
            } catch { problem = error.localizedDescription }
        }.onChange(of: phase) { _, phase in if phase == .active { Task { await refresh() } } }
    }

    func refresh() async { if let folder { await load(folder) } }

    func loadReview(_ url: URL, save: Bool = false) async {
        do {
            review = try await Task.detached { try SavedReview.load(from: url, save: save) }.value
        } catch {
            problem = "Couldn’t open this project review. " + error.localizedDescription
        }
    }

    func load(_ url: URL, remember: Bool = false) async {
        let token = UUID()
        refreshID = token
        do {
            let cards = try await Task.detached { try Workspace.projects(url) }.value
            guard refreshID == token else { return }
            if remember {
                let bookmark = try Workspace.access(url) { try url.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil) }
                UserDefaults.standard.set(bookmark, forKey: "workspace")
            }
            if folder != url { navigation = []; saved = false }
            projects = cards
            folder = url
        } catch { if refreshID == token { problem = error.localizedDescription } }
    }

    func capture() async {
        guard let folder, !saving else { return }
        saving = true
        defer { saving = false }
        let text = draft
        do {
            try await Task.detached { try Workspace.capture(text, in: folder) }.value
            draft = ""; problem = nil; capturing = false; saved = true
        } catch { problem = error.localizedDescription }
    }
}
