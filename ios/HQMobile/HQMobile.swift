import SwiftUI
import UniformTypeIdentifiers

@main
struct HQMobile: App {
    var body: some Scene { WindowGroup { ProjectsView() } }
}

private enum Destination: Hashable {
    case waiting, allProjects, todos
    case project(String)
}

private struct EditTarget {
    let title, revision: String
    var project: String?
    var status: String?

    init?(_ card: Card) {
        guard let revision = card.revision else { return nil }
        self.revision = revision; title = card.title; project = card.id; status = card.status
    }
    init?(_ todo: LiveHQ.Library.Todo) {
        guard let revision = todo.revision else { return nil }
        self.revision = revision; title = todo.title
    }
}

private struct ActionSheet: Identifiable {
    enum Kind { case move, later }
    let id = UUID()
    let target: EditTarget
    let kind: Kind
}

struct ProjectsView: View {
    @Environment(\.scenePhase) private var phase
    @State private var folder: URL?
    @State private var projects: [Card] = []
    @State private var todos: [LiveHQ.Library.Todo] = []
    @State private var routines: [LiveHQ.Library.Routine] = []
    @State private var statuses: [String] = []
    @State private var pending: [MobileEdit] = []
    @State private var issues: [String] = []
    @State private var actionSheet: ActionSheet?
    @State private var showingSettings = false
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
        let dates = projects.compactMap(\.availableFrom) + todos.compactMap { Workspace.availableFrom($0.deferred_until) }
            + pending.compactMap { Workspace.availableFrom($0.until) }
        let plateDates = todos.compactMap(\.plateFrom) + routines.compactMap(\.availableFrom)
        TimelineView(.explicit([Date.distantPast] + (dates + plateDates).sorted())) { _ in
            dashboard(asOf: .now)
        }
    }

    private func dashboard(asOf now: Date) -> some View {
        let projected = projects.map { project in
            var card = project
            if let edit = pending.first(where: { $0.project == card.id && $0.revision == card.revision }) {
                if let status = edit.status { card.status = status }
                if let until = edit.until { card.availableFrom = Workspace.availableFrom(until) }
            }
            return card
        }
        let visible = projected.filter { ($0.availableFrom ?? .distantPast) <= now }
        let availableTodos = todos.compactMap { value -> LiveHQ.Library.Todo? in
            var todo = value
            if let edit = pending.first(where: { $0.project == nil && $0.revision == todo.revision }) {
                if edit.action == "complete" { return nil }
                if let waiting = edit.waiting { todo.waiting = waiting }
                if let until = edit.until { todo.deferred_until = until }
            }
            return (Workspace.availableFrom(todo.deferred_until) ?? .distantPast) <= now ? todo : nil
        }
        return NavigationStack(path: $navigation) {
            List {
                if let connectionIssue {
                    Section {
                        Text(connectionIssue).font(.subheadline).foregroundStyle(.secondary)
                        if folder == nil || liveID == nil {
                            Button("Connect to HQ") { picking = true }
                        }
                    }
                }
                if !issues.isEmpty {
                    Section("Edits to review") {
                        ForEach(issues, id: \.self) { Text($0) }
                        Button("Dismiss") {
                            if let liveID { UserDefaults.standard.removeObject(forKey: LiveHQ.issuesKey(liveID)) }
                            issues = []
                        }
                    }
                }
                if liveID == nil, let review {
                    Section("Offline review · " + review.date.formatted(date: .abbreviated, time: .omitted)) {
                        projectRow(review.card, isReview: true)
                    }
                }
                if liveID != nil || (demo && folder != nil) {
                    plateSections(projects: visible, todos: availableTodos, now: now)
                }
            }
            .listStyle(.plain)
            .navigationTitle("My Plate")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Menu("Lists", systemImage: "list.bullet") {
                        Button("Waiting", systemImage: "hourglass") { navigation.append(.waiting) }
                        Button("Todos", systemImage: "checklist") { navigation.append(.todos) }
                        Button("All Projects", systemImage: "folder") { navigation.append(.allProjects) }
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Settings", systemImage: "gearshape") { showingSettings = true }
                }
            }
            .sheet(isPresented: $showingSettings) { MobileSettingsView() }
            .navigationDestination(for: Destination.self) { destination in
                switch destination {
                case .todos:
                    List {
                        ForEach([false, true], id: \.self) { waiting in
                            let group = availableTodos.filter { $0.waiting == waiting }
                            if !group.isEmpty {
                                Section(waiting ? "Waiting" : "Available") { ForEach(group) { todoRow($0) } }
                            }
                        }
                    }
                    .overlay { if availableTodos.isEmpty { ContentUnavailableView("No todos", systemImage: "checklist") } }
                    .listStyle(.plain).navigationTitle("Todos").refreshable { await refresh() }
                case .waiting, .allProjects:
                    let waiting = destination == .waiting
                    let cards = waiting ? visible.filter { $0.status == "waiting" || $0.status == "submitted" } : visible
                    List {
                        ForEach(cards) { card in projectRow(card) }
                    }
                    .listStyle(.plain)
                    .overlay { if cards.isEmpty { ContentUnavailableView("No projects here", systemImage: waiting ? "hourglass" : "folder") } }
                    .navigationTitle(waiting ? "Waiting" : "All Projects")
                    .refreshable { await refresh() }
                case .project(let id):
                    if let card = (review?.card.id == id ? review?.card : projected.first(where: { $0.id == id })) {
                        ProjectDetail(card: card, reviewDate: review?.card.id == id ? review?.date : nil)
                            .refreshable { await refresh() }
                            .toolbar {
                                if let target = editable(EditTarget(card)) {
                                    Menu("Project actions", systemImage: "ellipsis.circle") { deferButton(target); moveButton(target) }
                                }
                            }
                    } else { ContentUnavailableView("Project unavailable", systemImage: "doc") }
                }
            }
            .refreshable { await refresh() }
            .sheet(item: $actionSheet) { sheet in
                QuickActionSheet(sheet: sheet, statuses: statuses) { action, value, who in
                    submit(sheet.target, action, status: action == "move" ? value : nil,
                           who: who, until: action == "defer" ? value : nil)
                }
            }
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
                else if !pending.isEmpty { Label("\(pending.count) pending", systemImage: "icloud.and.arrow.up").font(.subheadline) }
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
                   let library = try? LiveHQ.Library.decode(cached) { display(library, id: id) }
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

    @ViewBuilder private func plateSections(projects: [Card], todos: [LiveHQ.Library.Todo], now: Date) -> some View {
        let cards = projects.filter { $0.status == "my-plate" }
        let due = todos.filter { ($0.plateFrom ?? .distantFuture) <= now }.sorted { ($0.due ?? "") < ($1.due ?? "") }
        let available = routines.filter { ($0.availableFrom ?? .distantFuture) <= now }
        if !due.isEmpty {
            Section("Todos") { ForEach(due) { todoRow($0, onPlate: true) } }
        }
        if !cards.isEmpty {
            Section("Projects") { ForEach(cards) { projectRow($0) } }
        }
        if !available.isEmpty {
            Section("Routines") {
                ForEach(available) { routine in
                    VStack(alignment: .leading, spacing: 3) {
                        Text(routine.title)
                        if let due = Workspace.availableFrom(routine.next_due) {
                            Text("Due " + due.formatted(date: .abbreviated, time: routine.next_due.count == 10 ? .omitted : .shortened))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        if cards.isEmpty && due.isEmpty && available.isEmpty {
            ContentUnavailableView("Nothing on your plate", systemImage: "checkmark.circle")
        }
    }

    private func display(_ library: LiveHQ.Library, id: UUID) {
        projects = library.cards
        todos = library.todos
        routines = library.routines ?? []
        statuses = library.statuses ?? []
        pending = (try? LiveHQ.pending(id)) ?? pending
        issues = (UserDefaults.standard.dictionary(forKey: LiveHQ.issuesKey(id)) as? [String: String] ?? [:]).values.sorted()
    }

    private func editable(_ target: EditTarget?) -> EditTarget? {
        guard let target, !pending.contains(where: {
            $0.project == target.project && (target.project != nil || $0.revision == target.revision)
        }) else { return nil }
        return target
    }

    private func submit(_ target: EditTarget, _ action: String, status: String? = nil, who: String? = nil,
                        until: String? = nil, waiting: Bool? = nil) {
        guard let liveID else { return }
        let edit = MobileEdit(workspace: liveID, id: UUID(), title: target.title, revision: target.revision,
                              action: action, project: target.project, status: status, waiting_on: who,
                              until: until, date: action == "complete" ? QuickActionSheet.day(.now) : nil, waiting: waiting)
        actionSheet = nil
        Task {
            do {
                try await Task.detached { try LiveHQ.save(edit) }.value
                pending = try LiveHQ.pending(liveID)
                saved = false
                await refresh()
            } catch { problem = error.localizedDescription }
        }
    }

    private func deferButton(_ target: EditTarget) -> some View {
        Button("Defer", systemImage: "calendar") { actionSheet = ActionSheet(target: target, kind: .later) }.tint(.orange)
    }

    @ViewBuilder private func deferSwipes(_ target: EditTarget) -> some View {
        Button("Tomorrow", systemImage: "sunrise") {
            submit(target, "defer", until: QuickActionSheet.day(QuickActionSheet.tomorrow))
        }.tint(.orange)
        deferButton(target)
    }

    private func moveButton(_ target: EditTarget) -> some View {
        Button("Move", systemImage: "arrow.right") { actionSheet = ActionSheet(target: target, kind: .move) }.tint(.blue)
    }

    private func moveButton(_ target: EditTarget, waiting: Bool) -> some View {
        Button(waiting ? "Available" : "Waiting", systemImage: waiting ? "tray" : "hourglass") {
            submit(target, "set_waiting", waiting: !waiting)
        }.tint(.purple)
    }

    private func todoRow(_ todo: LiveHQ.Library.Todo, onPlate: Bool = false) -> some View {
        let target = editable(EditTarget(todo))
        return HStack(alignment: .firstTextBaseline, spacing: 12) {
            Button { if let target { submit(target, "complete") } } label: { Image(systemName: "circle").foregroundStyle(.blue) }
                .buttonStyle(.borderless).accessibilityLabel("Complete " + todo.title).disabled(target == nil)
            VStack(alignment: .leading, spacing: 3) {
                Text(todo.title)
                if let due = todo.due {
                    Text((onPlate && todo.waiting ? "Waiting · Due " : "Due ") + due).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .swipeActions(edge: .leading) { if let target { deferSwipes(target) } }
        .swipeActions(edge: .trailing) { if let target { moveButton(target, waiting: todo.waiting) } }
        .contextMenu { if let target { deferButton(target); moveButton(target, waiting: todo.waiting) } }
    }

    private func projectRow(_ card: Card, isReview: Bool = false) -> some View {
        let target = editable(EditTarget(card))
        return NavigationLink(value: Destination.project(card.id)) {
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
        .swipeActions(edge: .leading) { if let target { deferSwipes(target) } }
        .swipeActions(edge: .trailing) { if let target { moveButton(target) } }
        .contextMenu { if let target { deferButton(target); moveButton(target) } }
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
            display(library, id: id)
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

private struct MobileSettingsView: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section("Swipe actions") {
                    LabeledContent("Swipe right", value: "Defer until tomorrow")
                    LabeledContent("Swipe left", value: "Move")
                }
                if let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String {
                    LabeledContent("HQ version", value: version)
                }
            }
            .navigationTitle("Settings")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}

private struct QuickActionSheet: View {
    let sheet: ActionSheet
    let statuses: [String]
    let apply: (String, String, String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var waiting = false
    @State private var who = ""
    static var tomorrow: Date { Calendar.current.dateInterval(of: .day, for: .now)!.end }
    @State private var date = Self.tomorrow

    static func day(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }

    var body: some View {
        NavigationStack {
            List {
                Section(sheet.target.title) {
                    if sheet.kind == .later {
                        Button("Tomorrow") { apply("defer", Self.day(Self.tomorrow), nil) }
                        Button("Next Monday") {
                            if let day = Calendar.current.nextDate(after: .now, matching: DateComponents(hour: 0, weekday: 2), matchingPolicy: .nextTime) {
                                apply("defer", Self.day(day), nil)
                            }
                        }
                        DatePicker("Choose date", selection: $date, in: Calendar.current.startOfDay(for: .now)..., displayedComponents: .date)
                        Button("Defer to " + date.formatted(date: .abbreviated, time: .omitted)) { apply("defer", Self.day(date), nil) }
                    } else if waiting {
                        TextField("Waiting on whom?", text: $who)
                        Button("Move to Waiting") { apply("move", "waiting", who) }
                            .disabled(who.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    } else {
                        ForEach(statuses.filter { $0 != sheet.target.status }, id: \.self) { status in
                            Button(status == "my-plate" ? "My Plate" : status.replacingOccurrences(of: "-", with: " ").capitalized) {
                                if status == "waiting" { waiting = true } else { apply("move", status, nil) }
                            }
                        }
                    }
                }
            }
            .navigationTitle(sheet.kind == .later ? "Defer" : "Move project")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
        .presentationDetents([.medium, .large])
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
