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

/// Atomically rewrites Claude Code's `~/.claude/settings.json` with timestamped backup of prior content.
/// Temp-then-rename prevents truncated/empty writes on crash, while timestamped backups ensure recoverable state.
fn write_settings(value: &Value) -> Result<(), String> {
    let path = settings_path().ok_or("Cannot resolve home dir")?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;

    if path.exists() {
        // Timestamped on every write, not once: "back up once" causes subsequent edits to overwrite the only backup (retired statusline bug).
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = path.with_file_name(format!("settings.json.aki-bak-{}", stamp));
        // A failed backup must not block the write the user asked for, but it is worth saying so.
        if let Err(e) = fs::copy(&path, &backup) {
            crate::logger::error(
                "claude_profile",
                &format!("could not back up settings.json before rewriting it: {}", e),
            );
        }
        // Pruned AFTER new copy exists so newest is kept; bounded retention avoids accumulating plaintext ANTHROPIC_AUTH_TOKEN credential copies.
        if let Some(dir) = path.parent() {
            crate::system::prune_timestamped_backups(
                dir,
                "settings.json.aki-bak-",
                crate::system::BACKUP_KEEP,
            );
        }
    }

    let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    crate::system::write_atomic(&path, &content)
}

/// Returns "proxy" if ANTHROPIC_BASE_URL is set under settings.json `env`, otherwise "native".
/// Claude Code only reads overrides via `env` at startup (top-level JSON keys like `customApiUrl` are ignored).
#[tauri::command]
pub fn get_claude_mode() -> &'static str {
    if read_settings()
        .get("env")
        .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str())
        .is_some()
    {
        "proxy"
    } else {
        "native"
    }
}

const PROXY_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
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

    let env_obj = obj
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("settings.json's env is not an object")?;

    for key in PROXY_ENV_KEYS { env_obj.remove(*key); }

    if mode == "proxy" {
        let fields: &[(&str, Option<String>)] = &[
            ("ANTHROPIC_BASE_URL",             endpoint),
            ("ANTHROPIC_AUTH_TOKEN",           api_key),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL",   model_opus),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL", model_sonnet),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL",  model_haiku),
        ];
        for (key, val) in fields {
            if let Some(v) = val { env_obj.insert(key.to_string(), v.clone().into()); }
        }
    }

    if env_obj.is_empty() {
        obj.remove("env");
    }

    write_settings(&settings)
}
