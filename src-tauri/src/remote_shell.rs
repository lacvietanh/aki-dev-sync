use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Hard ceiling for any `ssh host sh` call.
pub(crate) const REMOTE_SCRIPT_TIMEOUT_SECS: u64 = 30;

/// Hard LOCAL ceiling for the statusline probe/apply calls specifically.
pub(crate) const STATUSLINE_TIMEOUT_SECS: u64 = 5;

/// The self-timeout embedded in [`bounded_remote_sh`]'s wrapper.
pub(crate) const STATUSLINE_REMOTE_BOUND_SECS: u64 = 4;

#[derive(Clone, Copy)]
pub(crate) enum Shell {
    Plain,
    Bounded(u64),
}

/// Sends `script` to `ssh host sh` via stdin and returns the combined output.
pub(crate) fn run_remote_script(host: &str, script: &str) -> Result<Output, String> {
    run_remote_shell(host, Shell::Plain, "", script, REMOTE_SCRIPT_TIMEOUT_SECS)
}

/// Like [`run_remote_script`], but for statusline customizer's probe/apply calls.
pub(crate) fn run_remote_script_bounded(host: &str, script: &str) -> Result<Output, String> {
    run_remote_shell(
        host,
        Shell::Bounded(STATUSLINE_REMOTE_BOUND_SECS),
        "",
        script,
        STATUSLINE_TIMEOUT_SECS,
    )
}

/// Kills the remote/local process if it overruns `timeout_secs`, returning an explicit timeout error instead of blocking forever.
pub(crate) fn run_remote_shell(
    host: &str,
    shell: Shell,
    preamble: &str,
    script: &str,
    timeout_secs: u64,
) -> Result<Output, String> {
    let host_mutex = host_lock(host);
    let _host_guard = host_mutex.lock().unwrap_or_else(|e| e.into_inner());

    let local = is_local_host(host);
    let mut child = spawn_shell(shell, host, local)
        .map_err(|e| format!("Failed to spawn {}: {}", if local { "local process" } else { "SSH" }, e))?;

    let mut out_pipe = child.stdout.take().ok_or("stdout pipe missing")?;
    let mut err_pipe = child.stderr.take().ok_or("stderr pipe missing")?;
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    if let Some(mut stdin) = child.stdin.take() {
        if !preamble.is_empty() {
            stdin
                .write_all(preamble.as_bytes())
                .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        }
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
    }

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "script timed out after {}s (killed) host={}",
                        timeout_secs, host
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("Failed to poll script: {}", e)),
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Ok(Output { status, stdout, stderr })
}

fn spawn_shell(shell: Shell, host: &str, local: bool) -> std::io::Result<std::process::Child> {
    let mut cmd = match (shell, local) {
        (Shell::Plain, true) => Command::new("sh"),
        (Shell::Plain, false) => polling_ssh(host, "sh"),
        (Shell::Bounded(secs), true) => {
            let mut c = Command::new("sh");
            c.arg("-c").arg(bounded_remote_sh(secs));
            c
        }
        (Shell::Bounded(secs), false) => polling_ssh(host, &bounded_remote_sh(secs)),
    };
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

pub(crate) fn host_lock(host: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let registry = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = registry.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(host.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub(crate) fn is_local_host(host: &str) -> bool {
    host == "local" || host == "localhost"
}

pub(crate) fn polling_ssh(host: &str, remote_cmd: &str) -> Command {
    let mut c = Command::new("ssh");
    c.args([
        "-o", "BatchMode=yes",
        "-o", "ConnectTimeout=10",
        "-o", "ServerAliveInterval=5",
        "-o", "ServerAliveCountMax=3",
        host,
        remote_cmd,
    ]);
    c
}

fn bounded_remote_sh(timeout_secs: u64) -> String {
    format!(
        r#"if command -v timeout >/dev/null 2>&1; then exec timeout -k 1 {t} sh
elif command -v gtimeout >/dev/null 2>&1; then exec gtimeout -k 1 {t} sh
elif command -v perl >/dev/null 2>&1; then exec perl -e 'alarm shift; exec "sh"' {t}
else
  printf '[SHELL:bounded_remote_sh] WARNING no timeout/gtimeout/perl on this host - running unbounded\n' >&2
  exec sh
fi"#,
        t = timeout_secs
    )
}
