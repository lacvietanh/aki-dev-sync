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

/// Rewrites `~/.claude/settings.json` **atomically**, and keeps a timestamped copy of what was
/// there before.
///
/// This file is not ours. It is Claude Code's own settings - statusline config, permissions,
/// everything - and this function rewrites the whole document to change one key under `env`.
/// A plain `fs::write` truncates first: a crash or a full disk between truncate and write left the
/// user with an empty or half-written `settings.json` and no copy anywhere, which Claude Code
/// then reads as "no settings at all". Temp-then-rename means a reader sees either the whole old
/// file or the whole new one, and the backup means even a bad *value* is recoverable.
fn write_settings(value: &Value) -> Result<(), String> {
    let path = settings_path().ok_or("Cannot resolve home dir")?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;

    if path.exists() {
        // Timestamped on every write, not once ever: the "back up once" shape means every edit
        // after the first silently destroys the only copy - the exact bug already retired from
        // the statusline installer.
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
    }

    let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    crate::system::write_atomic(&path, &content)
}

/// Returns "proxy" if ANTHROPIC_BASE_URL is set under settings.json's `env` block, otherwise
/// "native". Claude Code only picks up API routing overrides via `env` (real environment
/// variables it reads at startup) - a top-level JSON key like the old `customApiUrl` is not
/// read by the CLI at all, so proxy mode silently no-op'd until this was fixed.
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
