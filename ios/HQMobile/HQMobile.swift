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
    case todos
    case project(String)
}

struct ProjectsView: View {
    @Environment(\.scenePhase) private var phase
    @State private var folder: URL?
    @State private var projects: [Card] = []
    @State private var todos: [LiveHQ.Library.Todo] = []
    @State private var picking = false
    @State private var liveID: UUID?
    @State private var connectionIssue: String?
    private let demo = ProcessInfo.processInfo.arguments.contains("--sample-workspace")
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
        let availableTodos = todos.filter { (Workspace.availableFrom($0.deferred_until) ?? .distantPast) <= now }
        return NavigationStack(path: $navigation) {
            List {
                if liveID != nil || (demo && folder != nil) {
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
                if liveID != nil {
                    NavigationLink(value: Destination.todos) {
                        HStack(spacing: 14) {
                            Image(systemName: "checklist").foregroundStyle(.blue).frame(width: 24)
                            Text("Todos").font(.headline)
                            Spacer()
                            Text(availableTodos.count, format: .number).foregroundStyle(.secondary).monospacedDigit()
                        }.padding(.vertical, 5)
                    }
                }
                if let connectionIssue {
                    Section {
                        Text(connectionIssue).font(.subheadline).foregroundStyle(.secondary)
                        if folder == nil || liveID == nil {
                            Button("Connect to HQ") { picking = true }
                        }
                    }
                }
                if liveID == nil, let review {
                    Section("Offline review · " + review.date.formatted(date: .abbreviated, time: .omitted)) {
                        projectRow(review.card, isReview: true)
                    }
                }
                if liveID != nil || (demo && folder != nil) {
                    Section {
                        ForEach(visible) { card in projectRow(card) }
                        if visible.isEmpty { Text("No projects to show.") }
                    } header: { Text("Projects") }
                }
            }
            .listStyle(.plain)
            .navigationTitle("HQ")
            .navigationDestination(for: Destination.self) { destination in
                switch destination {
                case .todos:
                    List(availableTodos) { todo in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(todo.title)
                            if todo.waiting { Text("Waiting").font(.caption).foregroundStyle(.secondary) }
                            if let due = todo.due { Text("Due " + due).font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                    .overlay { if availableTodos.isEmpty { ContentUnavailableView("No todos", systemImage: "checklist") } }
                    .listStyle(.plain).navigationTitle("Todos").refreshable { await refresh() }
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
            .refreshable { await refresh() }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
                do {
                    let url = try result.get()
                    Task { await load(url, remember: true) }
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
                            Text("Save to HQ Todos")
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
                if saved { Label(demo ? "Task saved to Todos" : "Capture queued for HQ Todos", systemImage: "checkmark.circle").font(.subheadline) }
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
                .disabled(liveID == nil && (!demo || folder == nil))
            }
            .padding(.horizontal, 20).padding(.vertical, 6)
        }
        .task {
            if !demo, let raw = UserDefaults.standard.string(forKey: "liveHQ"), let id = UUID(uuidString: raw) {
                liveID = id
                if let cached = try? Data(contentsOf: LiveHQ.cache(id)),
                   let library = try? LiveHQ.Library.decode(cached) { projects = library.cards; todos = library.todos }
            }
            if liveID == nil, FileManager.default.fileExists(atPath: SavedReview.location.path) {
                await loadReview(SavedReview.location)
            }
            do {
                if !demo, let bookmark = UserDefaults.standard.data(forKey: "workspace") {
                    var stale = false
                    let root = try URL(resolvingBookmarkData: bookmark, options: [], relativeTo: nil, bookmarkDataIsStale: &stale)
                    folder = root
                    await load(root, remember: stale)
                } else if demo {
                    let root = URL.documentsDirectory.appendingPathComponent("Sample HQ")
                    if !FileManager.default.fileExists(atPath: root.path) {
                        guard let sample = Bundle.main.url(forResource: "SampleHQ", withExtension: nil) else { throw CocoaError(.fileNoSuchFile) }
                        try FileManager.default.copyItem(at: sample, to: root)
                    }
                    projects = try Workspace.projects(root)
                    folder = root
                } else {
                    connectionIssue = "Choose HQ Live in iCloud Drive to connect this iPhone."
                }
            } catch { connectionIssue = "Reconnect to HQ Live in iCloud Drive." }
            while !demo && !Task.isCancelled {
                do { try await Task.sleep(for: .seconds(30)) } catch { return }
                await refresh()
            }
        }.onChange(of: phase) { _, phase in if phase == .active { Task { await refresh() } } }
    }

    func refresh() async { if !demo, let folder { await load(folder) } }

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
            let expected = liveID
            let (id, library) = try await Task.detached { try LiveHQ.refresh(url, expected: expected) }.value
            guard refreshID == token else { return }
            if remember {
                let bookmark = try Workspace.access(url) { try url.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil) }
                UserDefaults.standard.set(bookmark, forKey: "workspace")
            }
            if folder != url { navigation = []; saved = false }
            projects = library.cards
            todos = library.todos
            folder = url
            liveID = id
            UserDefaults.standard.set(id.uuidString, forKey: "liveHQ")
            connectionIssue = nil
        } catch {
            if refreshID == token {
                if let fileError = error as? CocoaError, fileError.code == .fileReadNoPermission {
                    folder = nil
                    connectionIssue = "Allow HQ to access HQ Live in iCloud Drive."
                } else {
                    connectionIssue = liveID == nil ? "Waiting for HQ Live in iCloud Drive." : "HQ is offline. Showing your last synced lists."
                }
            }
        }
    }

    func capture() async {
        guard !saving else { return }
        saving = true
        defer { saving = false }
        let text = draft
        do {
            if let id = liveID {
                try await Task.detached { try LiveHQ.capture(text, id: id) }.value
            } else if demo, let folder {
                try await Task.detached { try Workspace.capture(text, in: folder) }.value
            } else { return }
            draft = ""; problem = nil; capturing = false; saved = true
            await refresh()
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
            } else if let next = card.next {
                Section("Next action") { Text(next) }
            } else { Text("No saved overview.").foregroundStyle(.secondary) }
            DisclosureGroup("Full notes") { Text(card.notes).textSelection(.enabled) }
        }
        .listStyle(.plain)
        .navigationBarTitleDisplayMode(.inline)
    }
}
