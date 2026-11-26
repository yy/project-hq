import SwiftUI
import UniformTypeIdentifiers

@main
struct HQMobile: App {
    var body: some Scene { WindowGroup { ProjectsView() } }
}

private enum ProjectList: String, CaseIterable {
    case myPlate = "My Plate"
    case waiting = "Waiting"

    var symbol: String { self == .myPlate ? "star.fill" : "hourglass" }
    var color: Color { self == .myPlate ? .orange : .purple }

    func projects(in cards: [Card]) -> [Card] {
        cards.filter { card in
            switch self {
            case .myPlate: return card.status == "my-plate"
            case .waiting: return card.status == "waiting" || card.status == "submitted"
            }
        }
    }
}

private enum Destination: Hashable {
    case list(ProjectList)
    case project(String)
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
    @State private var navigation: [Destination] = []
    @State private var refreshID = UUID()
    @FocusState private var editingTitle: Bool

    var body: some View {
        TimelineView(.explicit([Date.distantPast] + projects.compactMap(\.availableFrom).sorted())) { _ in
            dashboard(asOf: .now)
        }
    }

    private func dashboard(asOf now: Date) -> some View {
        let visible = projects.filter { ($0.availableFrom ?? .distantPast) <= now }
        return NavigationStack(path: $navigation) {
            List {
                if folder != nil {
                    ForEach(ProjectList.allCases, id: \.self) { list in
                        NavigationLink(value: Destination.list(list)) {
                            HStack(spacing: 14) {
                                Image(systemName: list.symbol).foregroundStyle(list.color).frame(width: 24)
                                Text(list.rawValue).font(.headline)
                                Spacer()
                                Text(list.projects(in: visible).count, format: .number)
                                    .foregroundStyle(.secondary).monospacedDigit()
                            }.padding(.vertical, 5)
                        }
                    }
                }
                if let review {
                    Section("Offline review · " + review.date.formatted(date: .abbreviated, time: .omitted)) {
                        projectRow(review.card, isReview: true)
                    }
                }
                if let folder {
                    Section {
                        ForEach(visible) { card in projectRow(card) }
                        if visible.isEmpty { Text("No visible projects in this folder.") }
                    } header: { Text("Projects") } footer: { Text(folder.lastPathComponent) }
                }
            }
            .listStyle(.plain)
            .navigationTitle("HQ")
            .navigationDestination(for: Destination.self) { destination in
                switch destination {
                case .list(let list):
                    let cards = list.projects(in: visible)
                    List {
                        ForEach(cards) { card in projectRow(card) }
                    }
                    .listStyle(.plain)
                    .overlay { if cards.isEmpty { ContentUnavailableView("No projects here", systemImage: list.symbol) } }
                    .navigationTitle(list.rawValue)
                    .refreshable { await refresh() }
                case .project(let id):
                    if let card = (review?.card.id == id ? review?.card : projects.first(where: { $0.id == id })) {
                        ProjectDetail(card: card, reviewDate: review?.card.id == id ? review?.date : nil)
                            .refreshable { await refresh() }
                    } else { ContentUnavailableView("Project unavailable", systemImage: "doc") }
                }
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu("Open", systemImage: "folder") {
                        Button("Import project review") { pickingReview = true; picking = true }
                        Button("Open folder") { pickingReview = false; picking = true }
                    }
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
                        Section {
                            TextField("What needs doing?", text: $draft, axis: .vertical)
                                .focused($editingTitle)
                                .accessibilityIdentifier("capture-title")
                        } footer: {
                            if let folder { Text("Save to Todos in " + folder.lastPathComponent) }
                        }
                        if let problem { Text(problem).foregroundStyle(.red) }
                    }
                    .navigationTitle("Quick capture")
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) { Button("Cancel") { capturing = false }.disabled(saving) }
                        ToolbarItem(placement: .confirmationAction) { Button("Save") { Task { await capture() } }.disabled(saving || draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) }
                    }
                }
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
                .interactiveDismissDisabled(saving)
                .onAppear { editingTitle = true }
            }
            .alert("HQ", isPresented: Binding(get: { problem != nil && !capturing }, set: { if !$0 { problem = nil } })) {
                Button("OK") { problem = nil }
            } message: { Text(problem ?? "") }
        }
        .safeAreaInset(edge: .bottom, alignment: .trailing, spacing: 0) {
            HStack {
                if saved { Label("Task saved to Todos", systemImage: "checkmark.circle").font(.subheadline) }
                Spacer()
                Button {
                    saved = false
                    capturing = true
                } label: {
                    Image(systemName: "plus").font(.title2.weight(.semibold))
                        .foregroundStyle(.white).frame(width: 56, height: 56)
                        .background(Color.blue, in: Circle())
                        .shadow(color: .blue.opacity(0.20), radius: 6, y: 3)
                }
                .accessibilityLabel("Quick capture")
                .disabled(folder == nil)
            }
            .padding(.horizontal, 20).padding(.vertical, 6)
        }
        .task {
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

    private func projectRow(_ card: Card, isReview: Bool = false) -> some View {
        NavigationLink(value: Destination.project(card.id)) {
            HStack(alignment: .firstTextBaseline, spacing: 14) {
                Image(systemName: isReview ? "bookmark.fill" : "circle.dotted")
                    .foregroundStyle(isReview ? Color.teal : .blue).frame(width: 24)
                VStack(alignment: .leading, spacing: 4) {
                    Text(card.title).font(.headline)
                    if isReview, let summary = card.summary {
                        Text(summary.overview.actionTitle).font(.caption).foregroundStyle(.secondary)
                        Text(summary.overview.needs_you.text).font(.subheadline)
                    } else {
                        Text(card.statusLabel).font(.subheadline).foregroundStyle(.secondary)
                    }
                }
            }.padding(.vertical, 4)
        }
    }

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

private struct ProjectDetail: View {
    let card: Card
    let reviewDate: Date?

    var body: some View {
        List {
            VStack(alignment: .leading, spacing: 6) {
                Text(card.title).font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                Text(card.statusLabel).font(.subheadline).foregroundStyle(.secondary)
                if let reviewDate {
                    Text("Offline review · " + reviewDate.formatted(date: .abbreviated, time: .omitted))
                        .font(.caption).foregroundStyle(.secondary)
                }
            }.listRowSeparator(.hidden)
            if let summary = card.summary {
                Section(summary.overview.actionTitle) {
                    if card.stale { Label("Overview predates the latest edit", systemImage: "clock").font(.subheadline) }
                    Text(summary.overview.needs_you.text).font(.body.weight(.medium))
                }
                Section("Overview") {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(summary.overview.what_is.text)
                        Text(summary.overview.situation.text)
                    }
                }
            } else { Text("No saved overview.").foregroundStyle(.secondary) }
            DisclosureGroup("Full notes") { Text(card.notes).textSelection(.enabled) }
        }
        .listStyle(.plain)
        .navigationBarTitleDisplayMode(.inline)
    }
}
