import SwiftUI

struct AISettingsValues: Codable {
    var model: String
    var reasoning_effort: String
}

struct AIModelOption: Decodable {
    struct Effort: Decodable { let reasoningEffort: String }
    let model: String
    let displayName: String?
    let defaultReasoningEffort: String?
    let supportedReasoningEfforts: [Effort]?
}

struct AISettingsView: View {
    @ObservedObject var controller: ServerController
    @State private var values = AISettingsValues(model: "", reasoning_effort: "")
    @State private var models: [AIModelOption] = []
    @State private var working = false
    @State private var message = ""

    private var efforts: [String] {
        let supported = models.first { $0.model == values.model }?.supportedReasoningEfforts?.map(\.reasoningEffort) ?? []
        return supported.contains(values.reasoning_effort) ? supported : [values.reasoning_effort] + supported
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Picker("Default model", selection: $values.model) {
                if !models.contains(where: { $0.model == values.model }) {
                    Text(values.model.isEmpty ? "Loading…" : values.model).tag(values.model)
                }
                ForEach(models, id: \.model) { model in
                    Text(model.displayName ?? model.model).tag(model.model)
                }
            }
            .onChange(of: values.model) { _, model in
                guard let option = models.first(where: { $0.model == model }),
                      !(option.supportedReasoningEfforts ?? []).contains(where: { $0.reasoningEffort == values.reasoning_effort }) else { return }
                values.reasoning_effort = option.defaultReasoningEffort ?? "low"
            }
            Picker("Chat thinking", selection: $values.reasoning_effort) {
                ForEach(efforts, id: \.self) { effort in Text(effort.capitalized).tag(effort) }
            }
            Text("Used for new chats and project overviews. Changing a model inside a chat affects only that conversation.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Save AI defaults") { Task { await save() } }
                    .disabled(working || values.model.isEmpty)
                if working { ProgressView().controlSize(.small) }
                Text(message).font(.caption).foregroundStyle(.secondary)
            }
        }
        .disabled(working)
        .task(id: controller.appURL) { await load() }
    }

    private func load() async {
        working = true
        defer { working = false }
        do {
            let data = try await controller.apiData(path: "/api/ai/settings")
            values = try JSONDecoder().decode(AISettingsValues.self, from: data)
            struct Catalog: Decodable { let data: [AIModelOption] }
            if let catalog = try? await controller.apiData(path: "/api/agent/models") {
                models = (try? JSONDecoder().decode(Catalog.self, from: catalog).data) ?? []
            }
            message = ""
        } catch { message = error.localizedDescription }
    }

    private func save() async {
        working = true
        defer { working = false }
        do {
            _ = try await controller.apiData(path: "/api/ai/settings", body: JSONEncoder().encode(values))
            message = "Saved"
        } catch { message = error.localizedDescription }
    }
}
