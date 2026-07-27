use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionMode {
    Serial,
    #[default]
    Parallel,
}

impl ActionMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "serial" | "sequential" => Some(Self::Serial),
            "parallel" | "single" | "single-actions" => Some(Self::Parallel),
            _ => None,
        }
    }

    pub fn from_field(value: Option<&str>) -> Self {
        value.and_then(Self::parse).unwrap_or_default()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::Parallel => "parallel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionSource {
    MyNext,
    Checklist,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Action {
    pub text: String,
    pub contexts: Vec<String>,
    pub people: Vec<String>,
    pub completed: bool,
    pub available: bool,
    pub source: ActionSource,
    /// Zero-based line index within the Markdown body. `my_next` has no body line.
    pub line: Option<usize>,
}

impl Action {
    pub fn has_context(&self, context: &str) -> bool {
        normalize_context(context)
            .is_some_and(|context| self.contexts.iter().any(|candidate| candidate == &context))
    }

    pub fn has_person(&self, person: &str) -> bool {
        normalize_person(person)
            .is_some_and(|person| self.people.iter().any(|candidate| candidate == &person))
    }
}

fn valid_token_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '-' | '_' | '.')
}

fn normalize_token(value: &str, prefix: char) -> Option<String> {
    let value = value.trim().strip_prefix(prefix).unwrap_or(value.trim());
    (!value.is_empty() && value.chars().all(valid_token_character)).then(|| value.to_lowercase())
}

pub fn normalize_context(context: &str) -> Option<String> {
    normalize_token(context, '@')
}

pub fn normalize_person(person: &str) -> Option<String> {
    normalize_token(person, '&')
}

/// Compatibility alias for callers that used the original all-purpose tag API.
pub fn normalize_tag(tag: &str) -> Option<String> {
    normalize_context(tag)
}

fn prefixed_tokens(text: &str, prefix: char) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut tokens = Vec::new();

    for word in text.split_whitespace() {
        let Some(body) = word.strip_prefix(prefix) else {
            continue;
        };
        let captured: String = body
            .chars()
            .take_while(|character| valid_token_character(*character))
            .collect();
        let token = captured.trim_matches('.');
        let Some(token) = normalize_token(token, prefix) else {
            continue;
        };
        if seen.insert(token.clone()) {
            tokens.push(token);
        }
    }

    tokens
}

fn contexts_in(text: &str) -> Vec<String> {
    prefixed_tokens(text, '@')
        .into_iter()
        .filter(|context| !matches!(context.as_str(), "serial" | "parallel"))
        .collect()
}

fn people_in(text: &str) -> Vec<String> {
    prefixed_tokens(text, '&')
}

struct ListItem {
    indent: usize,
    action: Option<Action>,
    mode: Option<ActionMode>,
}

fn indentation_width(line: &str) -> usize {
    line.chars()
        .take_while(|character| character.is_whitespace())
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum()
}

fn mode_directive(text: &str) -> Option<ActionMode> {
    let directive = prefixed_tokens(text, '!')
        .iter()
        .rev()
        .find_map(|tag| match tag.as_str() {
            "serial" => Some(ActionMode::Serial),
            "parallel" => Some(ActionMode::Parallel),
            _ => None,
        });
    directive.or_else(|| {
        // Read the earlier @serial/@parallel syntax without treating those
        // structural markers as contexts.
        prefixed_tokens(text, '@')
            .iter()
            .rev()
            .find_map(|tag| match tag.as_str() {
                "serial" => Some(ActionMode::Serial),
                "parallel" => Some(ActionMode::Parallel),
                _ => None,
            })
    })
}

fn parse_list_item(line: &str, line_index: usize) -> Option<ListItem> {
    let indent = indentation_width(line);
    let trimmed = line.trim_start();
    let bullet = trimmed.as_bytes().first().copied()?;
    if !matches!(bullet, b'-' | b'*' | b'+') {
        return None;
    }

    let after_bullet = trimmed.get(1..)?;
    if !after_bullet.starts_with(char::is_whitespace) {
        return None;
    }
    let content = after_bullet.trim_start();
    if content.is_empty() {
        return None;
    }

    let Some(marker) = content.get(..3) else {
        return Some(ListItem {
            indent,
            action: None,
            mode: mode_directive(content),
        });
    };
    let completed = match marker.as_bytes() {
        [b'[', b' ', b']'] => Some(false),
        [b'[', b'x' | b'X', b']'] => Some(true),
        _ if content.starts_with('[') => return None,
        _ => None,
    };

    let Some(completed) = completed else {
        return Some(ListItem {
            indent,
            action: None,
            mode: mode_directive(content),
        });
    };
    let after_marker = content.get(3..)?;
    if !after_marker.starts_with(char::is_whitespace) {
        return None;
    }
    let text = after_marker.trim_start();
    if text.is_empty() {
        return None;
    }

    Some(ListItem {
        indent,
        action: Some(Action {
            text: text.to_string(),
            contexts: contexts_in(text),
            people: people_in(text),
            completed,
            available: false,
            source: ActionSource::Checklist,
            line: Some(line_index),
        }),
        mode: mode_directive(text),
    })
}

#[derive(Default)]
struct ActionNode {
    action_index: Option<usize>,
    mode: Option<ActionMode>,
    children: Vec<usize>,
}

fn branch_has_incomplete_action(
    node_index: usize,
    nodes: &[ActionNode],
    actions: &[Action],
) -> bool {
    nodes[node_index]
        .action_index
        .is_some_and(|index| !actions[index].completed)
        || nodes[node_index]
            .children
            .iter()
            .any(|child| branch_has_incomplete_action(*child, nodes, actions))
}

fn expose_available_actions(
    node_index: usize,
    inherited_mode: ActionMode,
    nodes: &[ActionNode],
    actions: &mut [Action],
) {
    let node = &nodes[node_index];
    let child_mode = node.mode.unwrap_or(inherited_mode);
    let incomplete_children: Vec<usize> = node
        .children
        .iter()
        .copied()
        .filter(|child| branch_has_incomplete_action(*child, nodes, actions))
        .collect();

    if !incomplete_children.is_empty() {
        match child_mode {
            ActionMode::Serial => {
                expose_available_actions(incomplete_children[0], child_mode, nodes, actions);
            }
            ActionMode::Parallel => {
                for child in incomplete_children {
                    expose_available_actions(child, child_mode, nodes, actions);
                }
            }
        }
    } else if let Some(index) = node.action_index {
        if !actions[index].completed {
            actions[index].available = true;
        }
    }
}

fn fence_marker(line: &str) -> Option<(u8, usize, &str)> {
    let trimmed = line.trim_start();
    let marker = trimmed.as_bytes().first().copied()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }

    let marker_count = trimmed
        .as_bytes()
        .iter()
        .take_while(|candidate| **candidate == marker)
        .count();
    (marker_count >= 3).then(|| (marker, marker_count, &trimmed[marker_count..]))
}

fn closes_fence(line: &str, marker: u8, opening_len: usize) -> bool {
    fence_marker(line).is_some_and(|(candidate, len, rest)| {
        candidate == marker && len >= opening_len && rest.trim().is_empty()
    })
}

pub fn parse_actions(
    body: &str,
    my_next: Option<&str>,
    mode: ActionMode,
    project_is_actionable: bool,
) -> Vec<Action> {
    let mut actions = Vec::new();

    if let Some(text) = my_next.map(str::trim).filter(|text| !text.is_empty()) {
        actions.push(Action {
            text: text.to_string(),
            contexts: contexts_in(text),
            people: people_in(text),
            completed: false,
            available: project_is_actionable,
            source: ActionSource::MyNext,
            line: None,
        });
    }

    let checklist_start = actions.len();
    let mut nodes = vec![ActionNode {
        mode: Some(mode),
        ..ActionNode::default()
    }];
    let mut parent_stack: Vec<(usize, usize)> = Vec::new();
    let mut open_fence = None;
    for (line_index, line) in body.lines().enumerate() {
        if let Some((marker, opening_len)) = open_fence {
            if closes_fence(line, marker, opening_len) {
                open_fence = None;
            }
            continue;
        }
        if let Some((marker, opening_len, _)) = fence_marker(line) {
            open_fence = Some((marker, opening_len));
            continue;
        }
        let Some(item) = parse_list_item(line, line_index) else {
            continue;
        };

        while parent_stack
            .last()
            .is_some_and(|(indent, _)| *indent >= item.indent)
        {
            parent_stack.pop();
        }
        let parent = parent_stack.last().map_or(0, |(_, index)| *index);
        let action_index = item.action.map(|action| {
            let index = actions.len();
            actions.push(action);
            index
        });
        let node_index = nodes.len();
        nodes.push(ActionNode {
            action_index,
            mode: item.mode,
            children: Vec::new(),
        });
        nodes[parent].children.push(node_index);
        parent_stack.push((item.indent, node_index));
    }

    if project_is_actionable && actions.len() > checklist_start {
        expose_available_actions(0, mode, &nodes, &mut actions);
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_context, normalize_person, normalize_tag, parse_actions, ActionMode, ActionSource,
    };

    #[test]
    fn parses_action_mode_values_and_aliases() {
        assert_eq!(ActionMode::parse("serial"), Some(ActionMode::Serial));
        assert_eq!(ActionMode::parse("parallel"), Some(ActionMode::Parallel));
        assert_eq!(ActionMode::parse("sequential"), Some(ActionMode::Serial));
        assert_eq!(ActionMode::parse("single"), Some(ActionMode::Parallel));
        assert_eq!(
            ActionMode::parse("single-actions"),
            Some(ActionMode::Parallel)
        );
        assert_eq!(ActionMode::parse("unknown"), None);
        assert_eq!(
            ActionMode::from_field(Some("unknown")),
            ActionMode::Parallel
        );
        assert_eq!(ActionMode::Serial.as_str(), "serial");
        assert_eq!(ActionMode::Parallel.as_str(), "parallel");
    }

    #[test]
    fn normalizes_context_and_person_queries() {
        assert_eq!(normalize_context("Phone"), Some("phone".to_string()));
        assert_eq!(normalize_context("@PHONE"), Some("phone".to_string()));
        assert_eq!(normalize_person("Alex"), Some("alex".to_string()));
        assert_eq!(normalize_person("&ALEX"), Some("alex".to_string()));
        assert_eq!(
            normalize_person("&youngho-eom"),
            Some("youngho-eom".to_string())
        );
        assert_eq!(normalize_context("@"), None);
        assert_eq!(normalize_person("&"), None);
        assert_eq!(normalize_context("two words"), None);
        assert_eq!(normalize_person("two words"), None);

        // The original API remains a context alias.
        assert_eq!(normalize_tag("Phone"), Some("phone".to_string()));
        assert_eq!(normalize_tag("@PHONE"), Some("phone".to_string()));
    }

    #[test]
    fn parses_contexts_people_and_mode_directives() {
        let actions = parse_actions(
            "- [ ] Call &Mom @Phone @quick\n  * [X] Sent mail to &Alex @email\n- Note\n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].text, "Call &Mom @Phone @quick");
        assert_eq!(actions[0].contexts, vec!["phone", "quick"]);
        assert_eq!(actions[0].people, vec!["mom"]);
        assert_eq!(actions[1].contexts, vec!["email"]);
        assert_eq!(actions[1].people, vec!["alex"]);
        assert!(actions[0].available);
        assert_eq!(actions[0].line, Some(0));
        assert!(actions[1].completed);
        assert!(!actions[1].available);
    }

    #[test]
    fn does_not_treat_email_addresses_as_contexts_or_people() {
        let actions = parse_actions(
            "- [ ] Email person@example.com @computer\n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert_eq!(actions[0].contexts, vec!["computer"]);
        assert!(actions[0].people.is_empty());
    }

    #[test]
    fn strips_sentence_punctuation_from_tokens() {
        let actions = parse_actions(
            "- [ ] Call &Alex, then work @home.\n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert_eq!(actions[0].people, vec!["alex"]);
        assert_eq!(actions[0].contexts, vec!["home"]);
    }

    #[test]
    fn ignores_malformed_or_empty_checkboxes() {
        let actions = parse_actions(
            "-[ ] Missing bullet space\n- [ ]Missing text space\n- [ ]   \n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert!(actions.is_empty());
    }

    #[test]
    fn ignores_checkbox_examples_inside_fenced_code_blocks() {
        let actions = parse_actions(
            "```markdown\n- [ ] Example @phone\n```\n- [ ] Real @computer\n\
             ~~~\n- [ ] Another example @errand\n~~~\n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].text, "Real @computer");
        assert_eq!(actions[0].line, Some(3));
    }

    #[test]
    fn serial_mode_exposes_only_first_incomplete_checklist_action() {
        let actions = parse_actions(
            "- [x] Done\n- [ ] First @computer\n- [ ] Later @phone\n",
            None,
            ActionMode::Serial,
            true,
        );

        assert!(!actions[0].available);
        assert!(actions[1].available);
        assert!(!actions[2].available);
    }

    #[test]
    fn parallel_mode_exposes_every_incomplete_action() {
        let actions = parse_actions("- [ ] One\n- [ ] Two\n", None, ActionMode::Parallel, true);
        assert!(actions.iter().all(|action| action.available));
    }

    #[test]
    fn serial_directive_exposes_only_the_first_action_in_its_branch() {
        let actions = parse_actions(
            "- Calls !serial\n  - [x] Mom\n  - [ ] Electrician @phone\n  - [ ] Plumber @phone\n\
             - [ ] Buy filter @errand\n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert_eq!(actions.len(), 4);
        assert!(!actions[0].available);
        assert!(actions[1].available);
        assert!(!actions[2].available);
        assert!(actions[3].available);
    }

    #[test]
    fn parallel_directive_can_override_an_inherited_serial_mode() {
        let actions = parse_actions(
            "- Shopping !parallel\n  - [ ] Buy filter\n  - [ ] Buy batteries\n\
             - [ ] Later top-level action\n",
            None,
            ActionMode::Serial,
            true,
        );

        assert_eq!(actions.len(), 3);
        assert!(actions[0].available);
        assert!(actions[1].available);
        assert!(!actions[2].available);
    }

    #[test]
    fn checkbox_with_children_is_a_group_until_its_children_are_complete() {
        let actions = parse_actions(
            "- [ ] Replace lights !serial\n  - [x] Buy fixtures\n  - [ ] Call electrician\n\
             - [ ] Other project\n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert_eq!(actions.len(), 4);
        assert!(!actions[0].available);
        assert!(!actions[1].available);
        assert!(actions[2].available);
        assert!(actions[3].available);
    }

    #[test]
    fn legacy_at_mode_markers_remain_readable_but_are_not_contexts() {
        let actions = parse_actions(
            "- Calls @serial\n  - [ ] First @phone\n  - [ ] Later @phone\n",
            None,
            ActionMode::Parallel,
            true,
        );

        assert!(actions[0].available);
        assert!(!actions[1].available);
        assert!(!actions[0].contexts.contains(&"serial".to_string()));
    }

    #[test]
    fn inactive_project_has_no_available_actions() {
        let actions = parse_actions(
            "- [ ] Body @phone\n",
            Some("Legacy @computer"),
            ActionMode::Parallel,
            false,
        );

        assert!(actions.iter().all(|action| !action.available));
    }

    #[test]
    fn my_next_remains_a_compatibility_action() {
        let actions = parse_actions("", Some("Call Julie @phone"), ActionMode::Parallel, true);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].source, ActionSource::MyNext);
        assert_eq!(actions[0].contexts, vec!["phone"]);
        assert!(actions[0].people.is_empty());
        assert!(actions[0].available);
        assert_eq!(actions[0].line, None);
    }
}
