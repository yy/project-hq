use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{Local, NaiveDate};

pub const TASKS_DIR: &str = "_tasks";
pub const TODO_FILE: &str = "_tasks/todo.txt";
pub const DONE_FILE: &str = "_tasks/done.txt";

#[derive(Debug)]
pub enum TaskError {
    InvalidPath(String),
    InvalidLine(String),
    Conflict,
    Read {
        file: String,
        source: std::io::Error,
    },
    Write {
        file: String,
        source: std::io::Error,
    },
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(file) => write!(f, "Invalid task path: {file}"),
            Self::InvalidLine(message) => write!(f, "Invalid task: {message}"),
            Self::Conflict => write!(f, "Task changed; reload and retry"),
            Self::Read { file, source } | Self::Write { file, source } => {
                write!(f, "{file}: {source}")
            }
        }
    }
}

impl std::error::Error for TaskError {}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Task {
    pub line: usize,
    pub raw: String,
    pub text: String,
    pub title: String,
    pub priority: Option<f64>,
    pub created: Option<NaiveDate>,
    pub completed: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    pub deferred_until: Option<NaiveDate>,
    pub waiting: bool,
    pub visible: bool,
    pub contexts: Vec<String>,
    pub people: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskInput {
    pub text: String,
    pub priority: Option<f64>,
    pub due: Option<NaiveDate>,
    pub deferred_until: Option<NaiveDate>,
    #[serde(default)]
    pub waiting: bool,
}

impl Task {
    fn parse(raw: &str, line: usize, today: NaiveDate) -> Result<Self, TaskError> {
        let tokens: Vec<&str> = raw.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(TaskError::InvalidLine("empty line".into()));
        }

        let mut cursor = 0;
        let completed = if tokens.first() == Some(&"x") {
            cursor += 1;
            let date = tokens
                .get(cursor)
                .and_then(|value| parse_date(value))
                .ok_or_else(|| {
                    TaskError::InvalidLine("completion date is required after x".into())
                })?;
            cursor += 1;
            Some(date)
        } else {
            None
        };
        let created = tokens.get(cursor).and_then(|value| parse_date(value));
        if created.is_some() {
            cursor += 1;
        }

        let mut content = Vec::new();
        let mut priority = None;
        let mut due = None;
        let mut deferred_until = None;
        let mut waiting = false;
        for token in &tokens[cursor..] {
            if let Some(value) = token.strip_prefix("p:") {
                let parsed = value
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        TaskError::InvalidLine("p: must contain a finite number".into())
                    })?;
                priority = Some(parsed);
            } else if let Some(value) = token.strip_prefix("due:") {
                due =
                    Some(parse_date(value).ok_or_else(|| {
                        TaskError::InvalidLine("due: must use YYYY-MM-DD".into())
                    })?);
            } else if let Some(value) = token.strip_prefix("t:") {
                deferred_until = Some(
                    parse_date(value)
                        .ok_or_else(|| TaskError::InvalidLine("t: must use YYYY-MM-DD".into()))?,
                );
            } else if *token == "status:waiting" {
                waiting = true;
            } else {
                content.push(*token);
            }
        }
        if content.is_empty() {
            return Err(TaskError::InvalidLine("text is required".into()));
        }

        let text = content.join(" ");
        let contexts = annotations(&content, '@');
        let people = annotations(&content, '&');
        let tags = annotations(&content, '+');
        let title = content
            .iter()
            .filter(|token| !token.starts_with(['@', '&', '+']))
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if title.is_empty() {
            return Err(TaskError::InvalidLine(
                "descriptive text is required".into(),
            ));
        }

        Ok(Self {
            line,
            raw: raw.to_string(),
            text,
            title,
            priority,
            created,
            completed,
            due,
            deferred_until,
            waiting,
            visible: completed.is_none() && deferred_until.is_none_or(|date| date <= today),
            contexts,
            people,
            tags,
        })
    }

    fn active_line(&self) -> String {
        format_task_line(
            self.created,
            &self.text,
            self.priority,
            self.due,
            self.deferred_until,
            self.waiting,
        )
    }
}

fn annotations(tokens: &[&str], prefix: char) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|token| token.strip_prefix(prefix))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn format_priority(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.6}");
        formatted.trim_end_matches('0').to_string()
    }
}

fn validate_input(input: &TaskInput) -> Result<(), TaskError> {
    let text = input.text.trim();
    if text.is_empty() || text.contains(['\n', '\r']) {
        return Err(TaskError::InvalidLine(
            "text must be one non-empty line".into(),
        ));
    }
    if input.priority.is_some_and(|value| !value.is_finite()) {
        return Err(TaskError::InvalidLine(
            "priority must be a finite number".into(),
        ));
    }
    Ok(())
}

fn format_task_line(
    created: Option<NaiveDate>,
    text: &str,
    priority: Option<f64>,
    due: Option<NaiveDate>,
    deferred_until: Option<NaiveDate>,
    waiting: bool,
) -> String {
    let mut parts = Vec::new();
    if let Some(created) = created {
        parts.push(created.to_string());
    }
    parts.push(text.trim().to_string());
    if let Some(priority) = priority {
        parts.push(format!("p:{}", format_priority(priority)));
    }
    if let Some(due) = due {
        parts.push(format!("due:{due}"));
    }
    if let Some(deferred_until) = deferred_until {
        parts.push(format!("t:{deferred_until}"));
    }
    if waiting {
        parts.push("status:waiting".into());
    }
    parts.join(" ")
}

pub fn resolve_task_path(hq_dir: &Path, file: &str) -> Result<PathBuf, TaskError> {
    if !matches!(file, TODO_FILE | DONE_FILE) {
        return Err(TaskError::InvalidPath(file.into()));
    }
    let relative = Path::new(file);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(TaskError::InvalidPath(file.into()));
    }
    let path = hq_dir.join(relative);
    let canonical_root = fs::canonicalize(hq_dir).unwrap_or_else(|_| hq_dir.to_path_buf());
    if let Ok(parent) = path.parent().unwrap_or(hq_dir).canonicalize() {
        if !parent.starts_with(canonical_root) {
            return Err(TaskError::InvalidPath(file.into()));
        }
    }
    Ok(path)
}

pub fn ensure_task_files(hq_dir: &Path) -> Result<(), TaskError> {
    let dir = hq_dir.join(TASKS_DIR);
    fs::create_dir_all(&dir).map_err(|source| TaskError::Write {
        file: TASKS_DIR.into(),
        source,
    })?;
    for file in [TODO_FILE, DONE_FILE] {
        let path = resolve_task_path(hq_dir, file)?;
        if !path.exists() {
            fs::write(&path, "").map_err(|source| TaskError::Write {
                file: file.into(),
                source,
            })?;
        }
    }
    Ok(())
}

pub fn read_task_text(hq_dir: &Path, file: &str) -> Result<String, TaskError> {
    let path = resolve_task_path(hq_dir, file)?;
    fs::read_to_string(path).map_err(|source| TaskError::Read {
        file: file.into(),
        source,
    })
}

pub(crate) fn validate_task_file_for_rewrite(hq_dir: &Path, file: &str) -> Result<(), TaskError> {
    let path = resolve_task_path(hq_dir, file)?;
    let metadata = fs::metadata(path).map_err(|source| TaskError::Read {
        file: file.into(),
        source,
    })?;
    if metadata.permissions().readonly() {
        return Err(TaskError::Write {
            file: file.into(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only file"),
        });
    }
    Ok(())
}

pub fn validate_task_text(file: &str, text: &str) -> Result<(), TaskError> {
    if !matches!(file, TODO_FILE | DONE_FILE) {
        return Err(TaskError::InvalidPath(file.into()));
    }
    let today = Local::now().date_naive();
    for (line, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let task = Task::parse(raw, line, today)?;
        if file == TODO_FILE && task.completed.is_some() {
            return Err(TaskError::InvalidLine(
                "completed lines belong in done.txt".into(),
            ));
        }
        if file == DONE_FILE && task.completed.is_none() {
            return Err(TaskError::InvalidLine(
                "done.txt lines must begin with x and a completion date".into(),
            ));
        }
    }
    Ok(())
}

pub fn write_task_text_if_unchanged(
    hq_dir: &Path,
    file: &str,
    expected: &str,
    replacement: &str,
) -> Result<(), TaskError> {
    validate_task_text(file, replacement)?;
    let path = resolve_task_path(hq_dir, file)?;
    let current = read_task_text(hq_dir, file)?;
    if current != expected {
        return Err(TaskError::Conflict);
    }
    fs::write(path, replacement).map_err(|source| TaskError::Write {
        file: file.into(),
        source,
    })
}

pub fn write_new_task_text(hq_dir: &Path, file: &str, text: &str) -> Result<(), TaskError> {
    validate_task_text(file, text)?;
    let path = resolve_task_path(hq_dir, file)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| TaskError::Write {
            file: file.into(),
            source,
        })?;
    }
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .and_then(|mut output| std::io::Write::write_all(&mut output, text.as_bytes()))
        .map_err(|source| TaskError::Write {
            file: file.into(),
            source,
        })
}

pub fn load_tasks(hq_dir: &Path) -> Result<Vec<Task>, TaskError> {
    let path = resolve_task_path(hq_dir, TODO_FILE)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = read_task_text(hq_dir, TODO_FILE)?;
    let today = Local::now().date_naive();
    let mut tasks = text
        .lines()
        .enumerate()
        .filter(|(_, raw)| !raw.trim().is_empty())
        .map(|(line, raw)| Task::parse(raw, line, today))
        .collect::<Result<Vec<_>, _>>()?;
    tasks.sort_by(|a, b| {
        b.priority
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&a.priority.unwrap_or(f64::NEG_INFINITY))
            .then_with(|| a.line.cmp(&b.line))
    });
    Ok(tasks)
}

fn task_at(hq_dir: &Path, line: usize, expected: &str) -> Result<(String, Task), TaskError> {
    let current = read_task_text(hq_dir, TODO_FILE)?;
    let raw = current
        .split('\n')
        .nth(line)
        .filter(|raw| *raw == expected)
        .ok_or(TaskError::Conflict)?;
    let task = Task::parse(raw, line, Local::now().date_naive())?;
    Ok((current, task))
}

pub fn create_task(hq_dir: &Path, input: &TaskInput) -> Result<Task, TaskError> {
    validate_input(input)?;
    ensure_task_files(hq_dir)?;
    let current = read_task_text(hq_dir, TODO_FILE)?;
    let created = Local::now().date_naive();
    let line = format_task_line(
        Some(created),
        &input.text,
        input.priority,
        input.due,
        input.deferred_until,
        input.waiting,
    );
    let mut replacement = current.clone();
    if !replacement.is_empty() && !replacement.ends_with('\n') {
        replacement.push('\n');
    }
    let line_index = replacement.lines().count();
    replacement.push_str(&line);
    replacement.push('\n');
    write_task_text_if_unchanged(hq_dir, TODO_FILE, &current, &replacement)?;
    Task::parse(&line, line_index, created)
}

pub fn update_task(
    hq_dir: &Path,
    line: usize,
    expected: &str,
    input: &TaskInput,
) -> Result<Task, TaskError> {
    validate_input(input)?;
    let (current, old) = task_at(hq_dir, line, expected)?;
    let replacement_line = format_task_line(
        old.created,
        &input.text,
        input.priority,
        input.due,
        input.deferred_until,
        input.waiting,
    );
    let mut lines: Vec<&str> = current.split('\n').collect();
    lines[line] = &replacement_line;
    let replacement = lines.join("\n");
    write_task_text_if_unchanged(hq_dir, TODO_FILE, &current, &replacement)?;
    Task::parse(&replacement_line, line, Local::now().date_naive())
}

pub fn set_task_priority(
    hq_dir: &Path,
    line: usize,
    expected: &str,
    priority: f64,
) -> Result<Task, TaskError> {
    if !priority.is_finite() {
        return Err(TaskError::InvalidLine(
            "priority must be a finite number".into(),
        ));
    }
    let (_, task) = task_at(hq_dir, line, expected)?;
    update_task(
        hq_dir,
        line,
        expected,
        &TaskInput {
            text: task.text,
            priority: Some(priority),
            due: task.due,
            deferred_until: task.deferred_until,
            waiting: task.waiting,
        },
    )
}

pub fn defer_task(
    hq_dir: &Path,
    line: usize,
    expected: &str,
    until: NaiveDate,
) -> Result<Task, TaskError> {
    let (_, task) = task_at(hq_dir, line, expected)?;
    update_task(
        hq_dir,
        line,
        expected,
        &TaskInput {
            text: task.text,
            priority: task.priority,
            due: task.due,
            deferred_until: Some(until),
            waiting: task.waiting,
        },
    )
}

pub fn complete_task(
    hq_dir: &Path,
    line: usize,
    expected: &str,
    completed: NaiveDate,
) -> Result<(), TaskError> {
    ensure_task_files(hq_dir)?;
    let (todo, task) = task_at(hq_dir, line, expected)?;
    let mut todo_lines: Vec<&str> = todo.split('\n').collect();
    todo_lines.remove(line);
    let todo_replacement = todo_lines.join("\n");
    let done = read_task_text(hq_dir, DONE_FILE)?;
    let mut done_replacement = done.clone();
    if !done_replacement.is_empty() && !done_replacement.ends_with('\n') {
        done_replacement.push('\n');
    }
    done_replacement.push_str(&format!("x {completed} {}\n", task.active_line()));

    write_task_text_if_unchanged(hq_dir, TODO_FILE, &todo, &todo_replacement)?;
    if let Err(error) = write_task_text_if_unchanged(hq_dir, DONE_FILE, &done, &done_replacement) {
        let _ = write_task_text_if_unchanged(hq_dir, TODO_FILE, &todo_replacement, &todo);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::NaiveDate;
    use tempfile::tempdir;

    use super::{
        complete_task, create_task, defer_task, load_tasks, set_task_priority, Task, TaskInput,
        DONE_FILE, TODO_FILE,
    };

    #[test]
    fn parses_concise_todo_txt_extensions() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 31).unwrap();
        let task = Task::parse(
            "2026-07-31 Call electrician @phone &electrician +house p:100 due:2026-08-15 t:2026-08-03 status:waiting",
            4,
            today,
        )
        .unwrap();
        assert_eq!(task.title, "Call electrician");
        assert_eq!(task.priority, Some(100.0));
        assert_eq!(task.contexts, ["phone"]);
        assert_eq!(task.people, ["electrician"]);
        assert_eq!(task.tags, ["house"]);
        assert!(!task.visible);
        assert!(task.waiting);
    }

    #[test]
    fn priority_updates_preserve_other_task_metadata() {
        let dir = tempdir().unwrap();
        let input = TaskInput {
            text: "Call electrician @phone +house".into(),
            priority: Some(100.0),
            due: NaiveDate::from_ymd_opt(2026, 8, 15),
            deferred_until: None,
            waiting: false,
        };
        let task = create_task(dir.path(), &input).unwrap();
        let updated = set_task_priority(dir.path(), task.line, &task.raw, 75.5).unwrap();
        assert_eq!(updated.priority, Some(75.5));
        assert_eq!(updated.due, input.due);
        assert!(updated.raw.contains("p:75.5"));
    }

    #[test]
    fn completion_moves_one_line_to_done_txt() {
        let dir = tempdir().unwrap();
        let first = create_task(
            dir.path(),
            &TaskInput {
                text: "First task +home".into(),
                priority: Some(20.0),
                due: None,
                deferred_until: None,
                waiting: false,
            },
        )
        .unwrap();
        create_task(
            dir.path(),
            &TaskInput {
                text: "Second task".into(),
                priority: Some(10.0),
                due: None,
                deferred_until: None,
                waiting: false,
            },
        )
        .unwrap();
        complete_task(
            dir.path(),
            first.line,
            &first.raw,
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        )
        .unwrap();
        let todo = fs::read_to_string(dir.path().join(TODO_FILE)).unwrap();
        let done = fs::read_to_string(dir.path().join(DONE_FILE)).unwrap();
        assert!(!todo.contains("First task"));
        assert!(todo.contains("Second task"));
        assert!(done.starts_with("x 2026-08-01 "));
        assert!(done.contains("First task +home p:20"));
    }

    #[test]
    fn loading_sorts_numeric_priority_high_to_low_and_defers_visibility() {
        let dir = tempdir().unwrap();
        let low = create_task(
            dir.path(),
            &TaskInput {
                text: "Low".into(),
                priority: Some(10.0),
                due: None,
                deferred_until: None,
                waiting: false,
            },
        )
        .unwrap();
        let high = create_task(
            dir.path(),
            &TaskInput {
                text: "High".into(),
                priority: Some(100.0),
                due: None,
                deferred_until: None,
                waiting: false,
            },
        )
        .unwrap();
        defer_task(
            dir.path(),
            low.line,
            &low.raw,
            NaiveDate::from_ymd_opt(2099, 1, 1).unwrap(),
        )
        .unwrap();
        let tasks = load_tasks(dir.path()).unwrap();
        assert_eq!(tasks[0].raw, high.raw);
        assert!(
            !tasks
                .iter()
                .find(|task| task.title == "Low")
                .unwrap()
                .visible
        );
    }
}
