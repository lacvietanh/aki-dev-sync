use crate::logger;
use std::process::Command;
use std::time::Duration;

const ANTIGRAVITY_APP_NAME: &str = "Antigravity IDE";

const ANTIGRAVITY_ACCOUNT_ONLY_PATHS: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Local Storage",
    "Session Storage",
    "Network Persistent State",
    "DIPS",
    "DIPS-wal",
    "TransportSecurity",
    "Trust Tokens",
    "Trust Tokens-journal",
];

const ANTIGRAVITY_AUTH_KEYS: &[&str] = &[
    "antigravityUnifiedStateSync.oauthToken",
    "antigravityUnifiedStateSync.userStatus",
];

fn antigravity_support_dir() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not resolve home directory".to_string())?;
    let home = std::path::PathBuf::from(home);

    #[cfg(target_os = "macos")]
    {
        Ok(home.join("Library/Application Support").join(ANTIGRAVITY_APP_NAME))
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA not set".to_string())?;
        Ok(std::path::PathBuf::from(appdata).join(ANTIGRAVITY_APP_NAME))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Ok(home.join(".config").join(ANTIGRAVITY_APP_NAME))
    }
}

fn remove_antigravity_auth_rows(base: &std::path::Path) {
    let where_in = ANTIGRAVITY_AUTH_KEYS
        .iter()
        .map(|k| format!("'{}'", k))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM ItemTable WHERE key IN ({});", where_in);

    for db_name in ["state.vscdb", "state.vscdb.backup"] {
        let db = base.join("User/globalStorage").join(db_name);
        if !db.is_file() {
            continue;
        }
        let out = Command::new("sqlite3")
            .arg(&db)
            .arg(&sql)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                logger::info("LOGOUT:antigravity", &format!("cleared {}", db_name));
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                logger::error("LOGOUT:antigravity", &format!("sqlite3 failed on {}: {}", db_name, err.trim()));
            }
            Err(e) => {
                logger::error("LOGOUT:antigravity", &format!("could not run sqlite3 on {}: {}", db_name, e));
            }
        }
    }
}

#[tauri::command]
pub async fn logout_antigravity() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("osascript")
                .args(["-e", &format!(r#"quit app "{}""#, ANTIGRAVITY_APP_NAME)])
                .output();
            std::thread::sleep(Duration::from_millis(800));
            let _ = Command::new("pkill").args(["-f", ANTIGRAVITY_APP_NAME]).output();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = Command::new("pkill").args(["-f", ANTIGRAVITY_APP_NAME]).output();
            std::thread::sleep(Duration::from_millis(800));
        }

        let base = antigravity_support_dir()?;
        for name in ANTIGRAVITY_ACCOUNT_ONLY_PATHS {
            let path = base.join(name);
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else if path.is_file() {
                let _ = std::fs::remove_file(&path);
            }
        }

        remove_antigravity_auth_rows(&base);

        #[cfg(target_os = "macos")]
        {
            let service = format!("{} Safe Storage", ANTIGRAVITY_APP_NAME);
            let _ = Command::new("security")
                .args(["delete-generic-password", "-s", &service])
                .output();
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[tauri::command]
pub async fn logout_antigravity_cli() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("pkill").args(["-f", "agy"]).output();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = Command::new("pkill").args(["-f", "agy"]).output();
        }

        if let Ok(home) = std::env::var("HOME") {
            let gemini_dir = std::path::Path::new(&home).join(".gemini");
            let target_files = ["oauth_creds.json", "google_accounts.json", "state.json"];
            for file_name in target_files {
                let file_path = gemini_dir.join(file_name);
                if file_path.is_file() {
                    let _ = std::fs::remove_file(&file_path);
                }
            }
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}
