use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Days, FixedOffset, Local, Months, NaiveDate};

use crate::frontmatter::parse_frontmatter_fields;
use crate::project::valid_deferred_until;
use crate::project_file::{
    format_frontmatter_value, project_body, resolve_project_path, slugify,
    write_markdown_text_if_unchanged,
};
use crate::sorted_markdown_files;

pub const ROUTINES_DIR: &str = "_routines";

#[derive(Debug)]
pub enum RoutineError {
    InvalidPath(String),
    InvalidField {
        field: &'static str,
        message: &'static str,
    },
    Read {
        file: String,
        source: std::io::Error,
    },
    Write {
        file: String,
        source: std::io::Error,
    },
    AlreadyExists(String),
    Malformed(String),
}

impl fmt::Display for RoutineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(file) => write!(f, "Invalid routine path: {file}"),
            Self::InvalidField { field, message } => write!(f, "Invalid {field}: {message}"),
            Self::Read { file, source } => write!(f, "{file}: {source}"),
            Self::Write { file, source } => write!(f, "{file}: {source}"),
            Self::AlreadyExists(file) => write!(f, "Routine already exists: {file}"),
            Self::Malformed(file) => write!(f, "Invalid routine frontmatter in {file}"),
        }
    }
}

impl std::error::Error for RoutineError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepeatFrom {
    Completion,
    Schedule,
}

impl RepeatFrom {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "completion" => Some(Self::Completion),
            "schedule" | "fixed" => Some(Self::Schedule),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Completion => "completion",
            Self::Schedule => "schedule",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineTiming {
    Deferred,
    Upcoming,
    Available,
    Due,
    Overdue,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Routine {
    pub title: String,
    pub area: String,
    pub repeat: String,
    pub repeat_from: RepeatFrom,
    pub available_before: String,
    pub next_due: NaiveDate,
    pub available_on: NaiveDate,
    pub last_completed: Option<NaiveDate>,
    pub deferred_until: Option<String>,
    pub timing: RoutineTiming,
    pub body: String,
    pub file: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RoutineInput {
    pub title: String,
    pub area: String,
    pub repeat: String,
    pub repeat_from: RepeatFrom,
    pub available_before: String,
    pub next_due: NaiveDate,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatUnit {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RepeatSpec {
    count: u32,
    unit: RepeatUnit,
}

impl RepeatSpec {
    fn parse(value: &str, allow_zero: bool) -> Option<Self> {
        let mut parts = value.split_whitespace();
        let count = parts.next()?.parse::<u32>().ok()?;
        if (!allow_zero && count == 0) || parts.clone().count() != 1 {
            return None;
        }
        let unit = match parts
            .next()?
            .trim_end_matches('s')
            .to_ascii_lowercase()
            .as_str()
        {
            "day" => RepeatUnit::Day,
            "week" => RepeatUnit::Week,
            "month" => RepeatUnit::Month,
            "year" => RepeatUnit::Year,
            _ => return None,
        };
        Some(Self { count, unit })
    }

    fn add_to(self, date: NaiveDate) -> Option<NaiveDate> {
        match self.unit {
            RepeatUnit::Day => date.checked_add_days(Days::new(self.count.into())),
            RepeatUnit::Week => date.checked_add_days(Days::new(u64::from(self.count) * 7)),
            RepeatUnit::Month => date.checked_add_months(Months::new(self.count)),
            RepeatUnit::Year => date.checked_add_months(Months::new(self.count.checked_mul(12)?)),
        }
    }

    fn subtract_from(self, date: NaiveDate) -> Option<NaiveDate> {
        match self.unit {
            RepeatUnit::Day => date.checked_sub_days(Days::new(self.count.into())),
            RepeatUnit::Week => date.checked_sub_days(Days::new(u64::from(self.count) * 7)),
            RepeatUnit::Month => date.checked_sub_months(Months::new(self.count)),
            RepeatUnit::Year => date.checked_sub_months(Months::new(self.count.checked_mul(12)?)),
        }
    }
}

impl Routine {
    pub fn from_text(text: &str, file: &str, today: NaiveDate) -> Result<Self, RoutineError> {
        let now = DateTime::from_naive_utc_and_offset(
            today.and_hms_opt(12, 0, 0).expect("noon is valid"),
            FixedOffset::east_opt(0).expect("UTC offset is valid"),
        );
        Self::from_text_at(text, file, now)
    }

    pub fn from_text_at(
        text: &str,
        file: &str,
        now: DateTime<FixedOffset>,
    ) -> Result<Self, RoutineError> {
        let fields =
            parse_frontmatter_fields(text).ok_or_else(|| RoutineError::Malformed(file.into()))?;
        if fields.get("type").map(String::as_str) != Some("routine") {
            return Err(RoutineError::Malformed(file.into()));
        }
        let title = required(&fields, "title")?;
        let area = fields
            .get("area")
            .cloned()
            .unwrap_or_else(|| "general".to_string());
        let repeat = required(&fields, "repeat")?;
        let repeat_spec = parse_interval("repeat", &repeat, false)?;
        let repeat_from = RepeatFrom::parse(&required(&fields, "repeat_from")?).ok_or(
            RoutineError::InvalidField {
                field: "repeat_from",
                message: "expected completion or schedule",
            },
        )?;
        let available_before = fields
            .get("available_before")
            .cloned()
            .unwrap_or_else(|| "0 days".to_string());
        let available_spec = parse_interval("available_before", &available_before, true)?;
        let next_due = parse_date(&fields, "next_due")?;
        let available_on =
            available_spec
                .subtract_from(next_due)
                .ok_or(RoutineError::InvalidField {
                    field: "available_before",
                    message: "date is out of range",
                })?;
        let last_completed = optional_date(&fields, "last_completed")?;
        let deferred_until = optional_deferred_until(&fields)?;
        let timing = timing_at(available_on, next_due, deferred_until.as_deref(), now);
        let _ = repeat_spec;

        Ok(Self {
            title,
            area,
            repeat,
            repeat_from,
            available_before,
            next_due,
            available_on,
            last_completed,
            deferred_until,
            timing,
            body: project_body(text).to_string(),
            file: file.to_string(),
        })
    }

    fn to_text(&self) -> String {
        routine_text(
            &RoutineInput {
                title: self.title.clone(),
                area: self.area.clone(),
                repeat: self.repeat.clone(),
                repeat_from: self.repeat_from,
                available_before: self.available_before.clone(),
                next_due: self.next_due,
                body: self.body.clone(),
            },
            self.last_completed,
            self.deferred_until.as_deref(),
        )
    }
}

pub fn load_routines(hq_dir: &Path) -> Vec<Routine> {
    let dir = hq_dir.join(ROUTINES_DIR);
    if !dir.is_dir() {
        return Vec::new();
    }
    let now = Local::now().fixed_offset();
    sorted_markdown_files(&dir, &[])
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let file = path.strip_prefix(hq_dir).ok()?.to_string_lossy();
            Routine::from_text_at(&text, &file, now).ok()
        })
        .collect()
}

pub fn validate_routine_text(file: &str, text: &str) -> Result<(), RoutineError> {
    routine_path_shape(file)?;
    Routine::from_text_at(text, file, Local::now().fixed_offset())?;
    Ok(())
}

pub fn write_new_routine_text(hq_dir: &Path, file: &str, text: &str) -> Result<(), RoutineError> {
    validate_routine_text(file, text)?;
    let path = routine_path(hq_dir, file)?;
    let parent = path
        .parent()
        .ok_or_else(|| RoutineError::InvalidPath(file.to_string()))?;
    fs::create_dir_all(parent).map_err(|source| RoutineError::Write {
        file: file.to_string(),
        source,
    })?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                RoutineError::AlreadyExists(file.to_string())
            } else {
                RoutineError::Write {
                    file: file.to_string(),
                    source,
                }
            }
        })?;
    output
        .write_all(text.as_bytes())
        .map_err(|source| RoutineError::Write {
            file: file.to_string(),
            source,
        })
}

pub fn write_routine_text_if_unchanged(
    hq_dir: &Path,
    file: &str,
    expected: &str,
    replacement: &str,
) -> Result<(), RoutineError> {
    validate_routine_text(file, replacement)?;
    write_markdown_text_if_unchanged(hq_dir, file, expected, replacement).map_err(|error| {
        RoutineError::Write {
            file: file.to_string(),
            source: std::io::Error::other(error.to_string()),
        }
    })
}

pub fn create_routine(hq_dir: &Path, input: &RoutineInput) -> Result<Routine, RoutineError> {
    validate_input(input)?;
    let dir = hq_dir.join(ROUTINES_DIR);
    fs::create_dir_all(&dir).map_err(|source| RoutineError::Write {
        file: ROUTINES_DIR.to_string(),
        source,
    })?;
    let slug = slugify(&input.title);
    if slug.is_empty() {
        return Err(RoutineError::InvalidField {
            field: "title",
            message: "must contain a letter or number",
        });
    }
    let file = next_filename(&dir, &slug);
    let relative = format!("{ROUTINES_DIR}/{file}");
    let text = routine_text(input, None, None);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(&file))
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::AlreadyExists {
                RoutineError::AlreadyExists(relative.clone())
            } else {
                RoutineError::Write {
                    file: relative.clone(),
                    source,
                }
            }
        })?;
    output
        .write_all(text.as_bytes())
        .map_err(|source| RoutineError::Write {
            file: relative.clone(),
            source,
        })?;
    Routine::from_text_at(&text, &relative, Local::now().fixed_offset())
}

pub fn update_routine(
    hq_dir: &Path,
    file: &str,
    input: &RoutineInput,
) -> Result<Routine, RoutineError> {
    validate_input(input)?;
    let current = read_routine(hq_dir, file)?;
    let text = routine_text(
        input,
        current.last_completed,
        current.deferred_until.as_deref(),
    );
    write_routine_text(hq_dir, file, &text)?;
    Routine::from_text_at(&text, file, Local::now().fixed_offset())
}

pub fn complete_routine(
    hq_dir: &Path,
    file: &str,
    today: NaiveDate,
) -> Result<Routine, RoutineError> {
    advance_routine(hq_dir, file, today, true)
}

pub fn skip_routine(hq_dir: &Path, file: &str, today: NaiveDate) -> Result<Routine, RoutineError> {
    advance_routine(hq_dir, file, today, false)
}

pub fn defer_routine(hq_dir: &Path, file: &str, until: &str) -> Result<Routine, RoutineError> {
    if !valid_deferred_until(until) {
        return Err(RoutineError::InvalidField {
            field: "deferred_until",
            message: "expected YYYY-MM-DD or an RFC 3339 timestamp",
        });
    }
    let mut routine = read_routine(hq_dir, file)?;
    routine.deferred_until = Some(until.to_string());
    let text = routine.to_text();
    write_routine_text(hq_dir, file, &text)?;
    Routine::from_text_at(&text, file, Local::now().fixed_offset())
}

fn advance_routine(
    hq_dir: &Path,
    file: &str,
    today: NaiveDate,
    completed: bool,
) -> Result<Routine, RoutineError> {
    let mut routine = read_routine(hq_dir, file)?;
    let repeat = parse_interval("repeat", &routine.repeat, false)?;
    routine.next_due = match routine.repeat_from {
        RepeatFrom::Completion => repeat.add_to(today),
        RepeatFrom::Schedule => {
            let mut next = routine.next_due;
            loop {
                next = repeat.add_to(next).ok_or(RoutineError::InvalidField {
                    field: "repeat",
                    message: "next date is out of range",
                })?;
                if next > today {
                    break Some(next);
                }
            }
        }
    }
    .ok_or(RoutineError::InvalidField {
        field: "repeat",
        message: "next date is out of range",
    })?;
    routine.deferred_until = None;
    if completed {
        routine.last_completed = Some(today);
    }
    routine.body = append_history(&routine.body, today, completed);
    let text = routine.to_text();
    write_routine_text(hq_dir, file, &text)?;
    Routine::from_text(&text, file, today)
}

fn append_history(body: &str, date: NaiveDate, completed: bool) -> String {
    let event = if completed { "completed" } else { "skipped" };
    let entry = format!("- {date} — {event}");
    let trimmed = body.trim_end();
    if trimmed.is_empty() {
        format!("## History\n\n{entry}\n")
    } else {
        let mut lines: Vec<&str> = trimmed.lines().collect();
        if let Some(history) = lines.iter().position(|line| line.trim() == "## History") {
            let insert_at = lines[history + 1..]
                .iter()
                .position(|line| line.starts_with("## "))
                .map(|offset| history + 1 + offset)
                .unwrap_or(lines.len());
            lines.insert(insert_at, &entry);
            format!("{}\n", lines.join("\n"))
        } else {
            format!("{trimmed}\n\n## History\n\n{entry}\n")
        }
    }
}

fn read_routine(hq_dir: &Path, file: &str) -> Result<Routine, RoutineError> {
    let path = routine_path(hq_dir, file)?;
    let text = fs::read_to_string(&path).map_err(|source| RoutineError::Read {
        file: file.to_string(),
        source,
    })?;
    Routine::from_text_at(&text, file, Local::now().fixed_offset())
}

fn write_routine_text(hq_dir: &Path, file: &str, text: &str) -> Result<(), RoutineError> {
    Routine::from_text_at(text, file, Local::now().fixed_offset())?;
    let path = routine_path(hq_dir, file)?;
    fs::write(path, text).map_err(|source| RoutineError::Write {
        file: file.to_string(),
        source,
    })
}

fn routine_path(hq_dir: &Path, file: &str) -> Result<PathBuf, RoutineError> {
    routine_path_shape(file)?;
    resolve_project_path(hq_dir, file).map_err(|_| RoutineError::InvalidPath(file.to_string()))
}

fn routine_path_shape(file: &str) -> Result<(), RoutineError> {
    let prefix = format!("{ROUTINES_DIR}/");
    let name = file
        .strip_prefix(&prefix)
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .ok_or_else(|| RoutineError::InvalidPath(file.to_string()))?;
    if !name.ends_with(".md") {
        return Err(RoutineError::InvalidPath(file.to_string()));
    }
    Ok(())
}

fn validate_input(input: &RoutineInput) -> Result<(), RoutineError> {
    validate_one_line("title", &input.title)?;
    validate_one_line("area", &input.area)?;
    parse_interval("repeat", &input.repeat, false)?;
    parse_interval("available_before", &input.available_before, true)?;
    Ok(())
}

fn validate_one_line(field: &'static str, value: &str) -> Result<(), RoutineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r']) {
        return Err(RoutineError::InvalidField {
            field,
            message: "must be one non-empty line",
        });
    }
    Ok(())
}

fn parse_interval(
    field: &'static str,
    value: &str,
    allow_zero: bool,
) -> Result<RepeatSpec, RoutineError> {
    RepeatSpec::parse(value, allow_zero).ok_or(RoutineError::InvalidField {
        field,
        message: "expected a number and day, week, month, or year",
    })
}

fn required(
    fields: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<String, RoutineError> {
    fields
        .get(field)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(RoutineError::InvalidField {
            field,
            message: "is required",
        })
}

fn parse_date(
    fields: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<NaiveDate, RoutineError> {
    NaiveDate::parse_from_str(&required(fields, field)?, "%Y-%m-%d").map_err(|_| {
        RoutineError::InvalidField {
            field,
            message: "expected YYYY-MM-DD",
        }
    })
}

fn optional_date(
    fields: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<Option<NaiveDate>, RoutineError> {
    fields
        .get(field)
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| RoutineError::InvalidField {
                field,
                message: "expected YYYY-MM-DD",
            })
        })
        .transpose()
}

fn optional_deferred_until(
    fields: &BTreeMap<String, String>,
) -> Result<Option<String>, RoutineError> {
    fields
        .get("deferred_until")
        .map(|value| {
            valid_deferred_until(value)
                .then(|| value.clone())
                .ok_or(RoutineError::InvalidField {
                    field: "deferred_until",
                    message: "expected YYYY-MM-DD or an RFC 3339 timestamp",
                })
        })
        .transpose()
}

fn timing_at(
    available_on: NaiveDate,
    next_due: NaiveDate,
    deferred_until: Option<&str>,
    now: DateTime<FixedOffset>,
) -> RoutineTiming {
    let today = now.date_naive();
    let deferred = deferred_until.is_some_and(|value| {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|date| date > today)
            .or_else(|_| DateTime::parse_from_rfc3339(value).map(|timestamp| timestamp > now))
            .unwrap_or(false)
    });
    if deferred {
        RoutineTiming::Deferred
    } else if available_on > today {
        RoutineTiming::Upcoming
    } else if next_due > today {
        RoutineTiming::Available
    } else if next_due == today {
        RoutineTiming::Due
    } else {
        RoutineTiming::Overdue
    }
}

fn routine_text(
    input: &RoutineInput,
    last_completed: Option<NaiveDate>,
    deferred_until: Option<&str>,
) -> String {
    let mut lines = vec![
        "type: routine".to_string(),
        format!("title: {}", format_frontmatter_value(input.title.trim())),
        format!("area: {}", format_frontmatter_value(input.area.trim())),
        format!("repeat: {}", format_frontmatter_value(input.repeat.trim())),
        format!("repeat_from: {}", input.repeat_from.as_str()),
        format!(
            "available_before: {}",
            format_frontmatter_value(input.available_before.trim())
        ),
        format!("next_due: {}", input.next_due),
    ];
    if let Some(date) = last_completed {
        lines.push(format!("last_completed: {date}"));
    }
    if let Some(value) = deferred_until {
        lines.push(format!("deferred_until: {value}"));
    }
    let body = input.body.trim_end();
    if body.is_empty() {
        format!("---\n{}\n---\n", lines.join("\n"))
    } else {
        format!("---\n{}\n---\n\n{body}\n", lines.join("\n"))
    }
}

fn next_filename(dir: &Path, slug: &str) -> String {
    for suffix in 1.. {
        let file = if suffix == 1 {
            format!("{slug}.md")
        } else {
            format!("{slug}-{suffix}.md")
        };
        if !dir.join(&file).exists() {
            return file;
        }
    }
    unreachable!("infinite range exhausted")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::{DateTime, FixedOffset, NaiveDate};
    use tempfile::tempdir;

    use super::{
        complete_routine, create_routine, defer_routine, load_routines, skip_routine, RepeatFrom,
        Routine, RoutineInput, RoutineTiming,
    };

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn instant(value: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(value).unwrap()
    }

    fn water_heater() -> RoutineInput {
        RoutineInput {
            title: "Flush water heater".into(),
            area: "home".into(),
            repeat: "1 year".into(),
            repeat_from: RepeatFrom::Completion,
            available_before: "1 month".into(),
            next_due: date("2027-07-30"),
            body: "Vendor: Example Plumbing".into(),
        }
    }

    #[test]
    fn parses_completion_based_annual_routine() {
        let routine = Routine::from_text(
            "---\n\
type: routine\n\
title: Flush water heater\n\
area: home\n\
repeat: 1 year\n\
repeat_from: completion\n\
available_before: 1 month\n\
next_due: 2027-07-30\n\
last_completed: 2026-07-30\n\
---\n\nVendor notes.\n",
            "_routines/flush-water-heater.md",
            date("2027-06-30"),
        )
        .unwrap();

        assert_eq!(routine.available_on, date("2027-06-30"));
        assert_eq!(routine.timing, RoutineTiming::Available);
        assert_eq!(routine.last_completed, Some(date("2026-07-30")));
    }

    #[test]
    fn completion_based_routine_advances_from_actual_completion() {
        let temp = tempdir().unwrap();
        let routine = create_routine(temp.path(), &water_heater()).unwrap();
        let completed = complete_routine(temp.path(), &routine.file, date("2027-08-02")).unwrap();

        assert_eq!(completed.next_due, date("2028-08-02"));
        assert_eq!(completed.last_completed, Some(date("2027-08-02")));
        assert!(completed.body.contains("2027-08-02 — completed"));
    }

    #[test]
    fn fixed_schedule_advances_to_one_future_occurrence_without_backlog() {
        let temp = tempdir().unwrap();
        let mut input = water_heater();
        input.repeat = "1 day".into();
        input.repeat_from = RepeatFrom::Schedule;
        input.next_due = date("2026-07-27");
        let routine = create_routine(temp.path(), &input).unwrap();
        let completed = complete_routine(temp.path(), &routine.file, date("2026-07-30")).unwrap();

        assert_eq!(completed.next_due, date("2026-07-31"));
        assert_eq!(completed.last_completed, Some(date("2026-07-30")));
    }

    #[test]
    fn skip_advances_without_changing_last_completion() {
        let temp = tempdir().unwrap();
        let routine = create_routine(temp.path(), &water_heater()).unwrap();
        let skipped = skip_routine(temp.path(), &routine.file, date("2027-07-30")).unwrap();

        assert_eq!(skipped.next_due, date("2028-07-30"));
        assert_eq!(skipped.last_completed, None);
        assert!(skipped.body.contains("2027-07-30 — skipped"));
    }

    #[test]
    fn completion_history_stays_inside_history_section() {
        let temp = tempdir().unwrap();
        let mut input = water_heater();
        input.body =
            "## History\n\n- 2026-07-30 — completed\n\n## Manual\n\nKeep this.".to_string();
        let routine = create_routine(temp.path(), &input).unwrap();
        let completed = complete_routine(temp.path(), &routine.file, date("2027-07-30")).unwrap();

        let new_entry = completed.body.find("2027-07-30 — completed").unwrap();
        let manual = completed.body.find("## Manual").unwrap();
        assert!(new_entry < manual);
    }

    #[test]
    fn deferral_hides_only_current_occurrence() {
        let temp = tempdir().unwrap();
        let routine = create_routine(temp.path(), &water_heater()).unwrap();
        let deferred = defer_routine(temp.path(), &routine.file, "2027-07-15").unwrap();

        let text = fs::read_to_string(temp.path().join(&routine.file)).unwrap();
        let parsed = Routine::from_text(&text, &routine.file, date("2027-07-01")).unwrap();
        assert_eq!(deferred.deferred_until.as_deref(), Some("2027-07-15"));
        assert_eq!(parsed.timing, RoutineTiming::Deferred);
    }

    #[test]
    fn timestamp_deferral_becomes_available_at_the_exact_instant() {
        let temp = tempdir().unwrap();
        let routine = create_routine(temp.path(), &water_heater()).unwrap();
        defer_routine(temp.path(), &routine.file, "2027-07-01T15:00:00-04:00").unwrap();
        let text = fs::read_to_string(temp.path().join(&routine.file)).unwrap();

        let deferred =
            Routine::from_text_at(&text, &routine.file, instant("2027-07-01T14:59:59-04:00"))
                .unwrap();
        let available =
            Routine::from_text_at(&text, &routine.file, instant("2027-07-01T15:00:00-04:00"))
                .unwrap();

        assert_eq!(deferred.timing, RoutineTiming::Deferred);
        assert_eq!(available.timing, RoutineTiming::Available);
    }

    #[test]
    fn routines_live_outside_project_tracks() {
        let temp = tempdir().unwrap();
        create_routine(temp.path(), &water_heater()).unwrap();
        let routines = load_routines(temp.path());
        assert_eq!(routines.len(), 1);
        assert_eq!(routines[0].file, "_routines/flush-water-heater.md");
    }
}
