//! Debug-build control socket for agents and scripts (docs/research/agent-control-channel.md § Decision); usage: docs/ref/agent-control.md.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::{app_paths, logger, web_server};

const SOCKET_NAME: &str = "control.sock";
const EVAL_TIMEOUT: Duration = Duration::from_secs(5);
const OPS: &str = "status, remote.on, remote.off, remote.status, eval";

/// Set by `npm run dev:agent`: the launch must never take the owner's focus.
pub fn agent_mode() -> bool {
    std::env::var_os("AKI_DEVSYNC_AGENT").is_some()
}

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    id: Value,
    op: String,
    #[serde(default)]
    arg: Option<String>,
}

fn socket_path() -> Result<PathBuf, String> {
    Ok(app_paths::app_data_dir()?.join(SOCKET_NAME))
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app).await {
            logger::error("CONTROL", &e);
        }
    });
}

pub fn shutdown() {
    if let Ok(path) = socket_path() {
        let _ = std::fs::remove_file(path);
    }
}

async fn serve(app: AppHandle) -> Result<(), String> {
    let path = socket_path()?;
    let _ = std::fs::remove_file(&path);
    let listener =
        UnixListener::bind(&path).map_err(|e| format!("bind {}: {}", path.display(), e))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod {}: {}", path.display(), e))?;
    logger::info("CONTROL", &format!("listening on {}", path.display()));
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("accept: {}", e))?;
        tauri::async_runtime::spawn(handle(stream, app.clone()));
    }
}

async fn handle(stream: UnixStream, app: AppHandle) {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(req) => {
                let id = req.id.clone();
                match dispatch(&app, req).await {
                    Ok(data) => json!({ "id": id, "ok": true, "data": data }),
                    Err(error) => json!({ "id": id, "ok": false, "error": error }),
                }
            }
            Err(e) => json!({ "id": null, "ok": false, "error": format!("bad request: {}", e) }),
        };
        if write
            .write_all(format!("{}\n", reply).as_bytes())
            .await
            .is_err()
        {
            return;
        }
    }
}

async fn dispatch(app: &AppHandle, req: Request) -> Result<Value, String> {
    match req.op.as_str() {
        "status" => Ok(json!({
            "version": app.package_info().version.to_string(),
            "identifier": app.config().identifier,
            "dataDir": app_paths::app_data_dir()?,
            "remote": remote_status().await?,
        })),
        "remote.on" => {
            web_server::start_companion_server().await?;
            remote_status().await
        }
        "remote.off" => {
            web_server::stop_companion_server().await?;
            remote_status().await
        }
        "remote.status" => remote_status().await,
        "eval" => {
            let body = req
                .arg
                .ok_or("eval needs arg: a JS function body, e.g. return document.title")?;
            eval(app, &body).await
        }
        other => Err(format!("unknown op '{}'; ops: {}", other, OPS)),
    }
}

async fn remote_status() -> Result<Value, String> {
    let mut status = serde_json::to_value(web_server::get_companion_status().await?)
        .map_err(|e| e.to_string())?;
    // The host token authenticates the app's own webview to the relay; no client of this socket needs it.
    if let Some(map) = status.as_object_mut() {
        map.remove("hostToken");
    }
    status["urls"] =
        serde_json::to_value(web_server::get_companion_url().await?).map_err(|e| e.to_string())?;
    Ok(status)
}

/// Runs a synchronous JS function body in the main webview; a returned Promise serializes as `{}`.
async fn eval(app: &AppHandle, body: &str) -> Result<Value, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    let script = format!(
        "(() => {{ try {{ const value = (() => {{ {body} }})(); return JSON.stringify({{ ok: true, value }}); }} catch (e) {{ return JSON.stringify({{ ok: false, error: String(e) }}); }} }})()"
    );
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = Mutex::new(Some(tx));
    window
        .eval_with_callback(script, move |raw| {
            if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = tx.send(raw);
            }
        })
        .map_err(|e| e.to_string())?;
    let raw = tokio::time::timeout(EVAL_TIMEOUT, rx)
        .await
        .map_err(|_| "eval returned nothing within 5 s: is the body valid JS?".to_string())?
        .map_err(|_| "eval callback dropped".to_string())?;
    decode_eval_reply(&raw)
}

/// The webview hands back the script's return value JSON-encoded, and that value is itself the JSON envelope built in `eval`.
fn decode_eval_reply(raw: &str) -> Result<Value, String> {
    let envelope: String =
        serde_json::from_str(raw).map_err(|e| format!("unexpected eval reply {}: {}", raw, e))?;
    let reply: Value = serde_json::from_str(&envelope).map_err(|e| e.to_string())?;
    if reply["ok"] == true {
        Ok(reply["value"].clone())
    } else {
        Err(reply["error"].as_str().unwrap_or("eval failed").to_string())
    }
}
