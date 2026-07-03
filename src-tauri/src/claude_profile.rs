use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn settings_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".claude/settings.json"))
}

fn read_settings() -> Value {
    settings_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

fn write_settings(value: &Value) -> Result<(), String> {
    let path = settings_path().ok_or("Cannot resolve home dir")?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(path, serde_json::to_string_pretty(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Returns "proxy" if customApiUrl is present in settings.json, otherwise "native".
#[tauri::command]
pub fn get_claude_mode() -> &'static str {
    if read_settings().get("customApiUrl").and_then(|v| v.as_str()).is_some() {
        "proxy"
    } else {
        "native"
    }
}

const PROXY_KEYS: &[&str] = &[
    "customApiUrl",
    "apiKey",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
];

#[tauri::command]
pub fn set_claude_profile(
    mode: String,
    endpoint:     Option<String>,
    api_key:      Option<String>,
    model_opus:   Option<String>,
    model_sonnet: Option<String>,
    model_haiku:  Option<String>,
) -> Result<(), String> {
    let mut settings = read_settings();
    let obj = settings.as_object_mut().ok_or("settings.json root is not an object")?;

    for key in PROXY_KEYS { obj.remove(*key); }

    if mode == "proxy" {
        let fields: &[(&str, Option<String>)] = &[
            ("customApiUrl",                   endpoint),
            ("apiKey",                         api_key),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL",   model_opus),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL", model_sonnet),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL",  model_haiku),
        ];
        for (key, val) in fields {
            if let Some(v) = val { obj.insert(key.to_string(), v.clone().into()); }
        }
    }

    write_settings(&settings)
}
