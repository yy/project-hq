//! HQ-owned preferences, independent of Codex configuration and localhost ports.
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const DEFAULT_MODEL: &str = "gpt-6-astra";
pub const DEFAULT_EFFORT: &str = "xhigh";

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct AiSettings {
    #[serde(alias = "chat_model")]
    pub model: String,
    #[serde(alias = "chat_effort")]
    pub reasoning_effort: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.into(),
            reasoning_effort: DEFAULT_EFFORT.into(),
        }
    }
}

impl AiSettings {
    fn validate(&self) -> Result<(), String> {
        for value in [&self.model, &self.reasoning_effort] {
            if value.is_empty()
                || value.len() > 128
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte))
            {
                return Err("Choose an explicit model and thinking level.".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct AiSettingsStore {
    path: Option<PathBuf>,
    lock: Arc<Mutex<()>>,
}

impl Default for AiSettingsStore {
    fn default() -> Self {
        let root = application_support_dir();
        Self {
            path: root.map(|root| root.join("net.yulab.project-hq/ai-settings.json")),
            lock: Arc::new(Mutex::new(())),
        }
    }
}

pub(crate) fn application_support_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                PathBuf::from(home).join(if cfg!(target_os = "macos") {
                    "Library/Application Support"
                } else {
                    ".config"
                })
            })
        })
}

impl AiSettingsStore {
    fn load(&self) -> Result<AiSettings, String> {
        let Some(path) = &self.path else {
            return Ok(AiSettings::default());
        };
        match fs::read(path) {
            Ok(bytes) => {
                let settings: AiSettings = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("Could not read HQ AI settings: {error}"))?;
                settings.validate()?;
                Ok(settings)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AiSettings::default()),
            Err(error) => Err(format!("Could not read HQ AI settings: {error}")),
        }
    }

    pub fn read(&self) -> Result<AiSettings, String> {
        let _lock = self.lock.lock().unwrap();
        self.load()
    }

    pub fn update(&self, change: impl FnOnce(&mut AiSettings)) -> Result<AiSettings, String> {
        let _lock = self.lock.lock().unwrap();
        let mut settings = self.load()?;
        change(&mut settings);
        settings.validate()?;
        let path = self
            .path
            .as_ref()
            .ok_or("HQ's settings directory is unavailable.")?;
        let save = || -> Result<(), Box<dyn std::error::Error>> {
            let parent = path.parent().ok_or("Missing settings parent directory")?;
            fs::create_dir_all(parent)?;
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            serde_json::to_writer_pretty(&mut temp, &settings)?;
            temp.persist(path)?;
            Ok(())
        };
        save().map_err(|error| format!("Could not save HQ AI settings: {error}"))?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_survives_restart_and_migrates_the_saved_chat_choice() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ai-settings.json");
        let store = AiSettingsStore {
            path: Some(path.clone()),
            lock: Arc::new(Mutex::new(())),
        };
        assert_eq!(store.read().unwrap().model, DEFAULT_MODEL);
        fs::write(
            &path,
            r#"{"chat_model":"saved-chat","chat_effort":"high","overview_model":"old-overview"}"#,
        )
        .unwrap();
        assert_eq!(store.read().unwrap().model, "saved-chat");
        store
            .update(|settings| settings.model = "new-default".into())
            .unwrap();
        let restarted = AiSettingsStore {
            path: Some(path.clone()),
            lock: Arc::new(Mutex::new(())),
        };
        let saved = restarted.read().unwrap();
        assert_eq!(saved.model, "new-default");
        assert_eq!(saved.reasoning_effort, "high");
        assert!(!fs::read_to_string(path).unwrap().contains("overview_model"));
        assert!(restarted.update(|settings| settings.model.clear()).is_err());
        assert_eq!(restarted.read().unwrap().model, "new-default");
    }

    #[test]
    fn corrupt_settings_are_reported_instead_of_switching_models() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ai-settings.json");
        fs::write(&path, "invalid json").unwrap();
        let store = AiSettingsStore {
            path: Some(path),
            lock: Arc::new(Mutex::new(())),
        };
        assert!(store.read().is_err());
    }
}
