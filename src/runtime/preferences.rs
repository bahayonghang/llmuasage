//! CLI preferences are independent of Store initialization and the worker lock.
use std::{
    fs::{self, OpenOptions},
    path::Path,
};

use anyhow::{Context, Result};
use fs4::FileExt;
use serde_json::{Map, Value};

use crate::common::i18n::Language;

pub const SETTINGS_FILE: &str = "cli-settings.json";
pub const LOCK_FILE: &str = "cli-settings.lock";

#[derive(Debug)]
pub struct Preferences {
    pub language: Language,
    pub diagnostic: Option<PreferenceDiagnostic>,
}

#[derive(Debug)]
pub enum PreferenceDiagnostic {
    Read(std::io::Error),
    Json(serde_json::Error),
    ExpectedObject,
    InvalidLanguage,
    ExpectedLanguageString,
}

impl PreferenceDiagnostic {
    pub fn render(&self, language: Language) -> String {
        match self {
            Self::Read(error) => error.to_string(),
            Self::Json(error) => error.to_string(),
            Self::ExpectedObject => language
                .select("Expected a JSON object", "配置必须是 JSON 对象")
                .into(),
            Self::InvalidLanguage => language
                .select("Language must be en or zh", "语言必须是 en 或 zh")
                .into(),
            Self::ExpectedLanguageString => language
                .select("Language must be a string", "语言必须是字符串")
                .into(),
        }
    }
}

/// A missing preference is normal. Other failures are reported once by the caller.
pub fn read(root: &Path) -> Preferences {
    let path = root.join(SETTINGS_FILE);
    let result = (|| -> std::result::Result<Language, PreferenceDiagnostic> {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Language::English);
            }
            Err(error) => return Err(PreferenceDiagnostic::Read(error)),
        };
        let document: Value = serde_json::from_slice(&bytes).map_err(PreferenceDiagnostic::Json)?;
        let object = document
            .as_object()
            .ok_or(PreferenceDiagnostic::ExpectedObject)?;
        match object.get("language") {
            None => Ok(Language::English),
            Some(Value::String(code)) => {
                Language::parse(code).ok_or(PreferenceDiagnostic::InvalidLanguage)
            }
            Some(_) => Err(PreferenceDiagnostic::ExpectedLanguageString),
        }
    })();
    match result {
        Ok(language) => Preferences {
            language,
            diagnostic: None,
        },
        Err(error) => Preferences {
            language: Language::English,
            diagnostic: Some(error),
        },
    }
}

/// Serializes recovery and replacement under an independent OS lock. Readers never lock.
pub fn write(root: &Path, language: Language) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("{}", root.display()))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(LOCK_FILE))?;
    FileExt::lock(&lock)?;
    let path = root.join(SETTINGS_FILE);
    // Recovery must precede reading the document, inside the same writer lock.
    crate::integrations::recover_and_cleanup_residue(&path)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut object = bytes
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_else(Map::new);
    object.insert("language".into(), Value::String(language.code().into()));
    crate::integrations::write_file_atomic(&path, serde_json::to_vec_pretty(&object)?)?;
    Ok(())
}
