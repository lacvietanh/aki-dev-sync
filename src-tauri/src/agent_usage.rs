// @docs docs/arch/usage-claudecode.md
// @docs docs/arch/usage-antigravity.md
// Updated: 2026-06-25 (v1.3.3 logging + SSH-script resilience: timeout + error surfacing + log levels)

use crate::logger;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Hard ceiling for any `ssh host sh` call. A hung `claude auth status` (network/API stall) must never wedge the UI in a permanent "loading" state (see docs/arch/usage-claudecode.md §2).
const REMOTE_SCRIPT_TIMEOUT_SECS: u64 = 30;

/// Per-`claude` bound enforced ON THE REMOTE (see [`CLAUDE_BIN_RESOLVER_PREAMBLE`]). Only `claude auth status` still runs through this preamble - the usage-fetch flow no longer spawns `claude` at all (see docs/arch/usage-claudecode.md §5).
const CLAUDE_CALL_TIMEOUT_SECS: u64 = 45;

/// Hard LOCAL ceiling for the statusline probe/apply calls specifically. That work is a handful of local file-system calls on the remote (mkdir/cat/chmod/jq) - nothing like the network/API round-trip `claude auth status` needs 30-45s for - so there is nothing to gain from waiting anywhere near REMOTE_SCRIPT_TIMEOUT_SECS, and everything to lose: a stuck host held the whole Customizer "busy" for up to 30s per host before this was split out.
const STATUSLINE_TIMEOUT_SECS: u64 = 5;

/// The self-timeout embedded in [`bounded_remote_sh`]'s wrapper - kept under STATUSLINE_TIMEOUT_SECS so the remote/local shell has already terminated itself by the time this module's own clock runs out, instead of the two racing each other.
const STATUSLINE_REMOTE_BOUND_SECS: u64 = 4;

/// Sends `script` to `ssh host sh` via stdin and returns the combined output.
pub(crate) fn run_remote_script(host: &str, script: &str) -> Result<Output, String> {
    run_interpreter_timeout(host, Interpreter::Sh, script, REMOTE_SCRIPT_TIMEOUT_SECS)
}

/// Like [`run_remote_script`] but for the Antigravity usage probe, which is a `node` script (not POSIX `sh`) piped over the same funnel. Generalizes the timeout/kill/drain machinery instead of duplicating it - AG's IPC previously had no timeout at all, so a blackholed SSH/local probe wedged `isChecking` permanently.
pub(crate) fn run_remote_node_timeout(host: &str, script: &str) -> Result<Output, String> {
    run_interpreter_timeout(host, Interpreter::Node, script, REMOTE_SCRIPT_TIMEOUT_SECS)
}

/// Like [`run_remote_script`], but for the statusline customizer's probe/apply calls: a much shorter local ceiling ([`STATUSLINE_TIMEOUT_SECS`]), and the remote/local shell itself is wrapped in a self-terminating timeout (see [`bounded_remote_sh`]) so a killed local SSH client can never leave an orphaned remote process running unbounded.
pub(crate) fn run_remote_script_bounded(host: &str, script: &str) -> Result<Output, String> {
    run_interpreter_timeout(
        host,
        Interpreter::BoundedSh(STATUSLINE_REMOTE_BOUND_SECS),
        script,
        STATUSLINE_TIMEOUT_SECS,
    )
}

/// Returns the shared lock for `host`, creating it on first use. Held for the duration of one [`run_interpreter_timeout`] call so that no two remote-script invocations - regardless of which feature triggered them (usage polling, git info, statusline probe/apply, ...) - ever run concurrently against the same host.
///
/// WHY: a burst of overlapping SSH connections to one host (e.g. the statusline auto-install probe firing while the user manually clicks Apply) can, over a constrained network path, stall *other*, unrelated SSH sessions to that same host for a moment even though nothing is actually killed - this serializes this app's own traffic to each host so it can never be the cause of that. Different hosts are unaffected - each gets its own independent lock, so per-host parallelism (see `apply_statusline_config`) is untouched.
fn host_lock(host: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let registry = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = registry.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(host.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn is_local_host(host: &str) -> bool {
    host == "local" || host == "localhost"
}

/// Every `ssh` this module spawns on a timer goes through here. Without these options an SSH to a saturated host can burn the entire 30s script budget on the TCP/auth handshake alone (nothing has run remotely yet, yet we time out, kill, and re-spawn on the next tick), and a blackholed connection never returns at all because the kernel's default TCP timeout is minutes long. `BatchMode` additionally guarantees we never block on a password prompt.
///
/// See docs/research/claudecode-usage-FINAL.md §4.
fn polling_ssh(host: &str, remote_cmd: &str) -> Command {
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

/// Which interpreter to invoke for a given probe, and how - each script family needs a different local/remote invocation and prelude (a POSIX-sh CLAUDE_BIN preamble is invalid JS, so it must never be sent ahead of a `node` script).
#[derive(Clone, Copy)]
enum Interpreter {
    /// CC: local `sh`, remote `ssh host sh`. Gets [`CLAUDE_BIN_RESOLVER_PREAMBLE`] prepended.
    Sh,
    /// AG: node, local and remote alike, resolved through the single shared [`NODE_BIN_RESOLVER_PREAMBLE`] - baked into the command the shell is handed (`zsh -lc <resolver>` locally, the ssh *remote command itself* remotely) rather than into the stdin preamble mechanism; see that constant's doc comment for why stdin is not usable here. `preamble()` still returns "" for this variant; the resolver is not stdin content.
    Node,
    /// Like `Sh`, but the local/remote shell invocation itself is wrapped in [`bounded_remote_sh`]'s self-timeout (the carried `u64`, in seconds) instead of relying solely on this module's own kill-after-timeout on the *local* `ssh`/`sh` process. See [`run_remote_script_bounded`] for why. No preamble - statusline scripts never invoke `claude`.
    BoundedSh(u64),
}

impl Interpreter {
    fn spawn(self, host: &str, local: bool) -> std::io::Result<std::process::Child> {
        let mut cmd = match (self, local) {
            (Interpreter::Sh, true) => Command::new("sh"),
            (Interpreter::Sh, false) => polling_ssh(host, "sh"),
            (Interpreter::BoundedSh(secs), true) => {
                let mut c = Command::new("sh");
                c.arg("-c").arg(bounded_remote_sh(secs));
                c
            }
            (Interpreter::BoundedSh(secs), false) => polling_ssh(host, &bounded_remote_sh(secs)),
            (Interpreter::Node, true) => {
                // Same resolver string as the remote branch, run through the local login shell instead of sshd's `$SHELL -c`. `zsh -lc` is kept only as the OUTER shell so the login PATH is still available to the resolver's last-resort `command -v node`; the static `[ -x path ]` candidates inside run first and do not depend on rc-sourcing having finished, which is what removes the cold-start race (stack-tauri A2). Passing the constant verbatim works because it is already a self-contained one-line `sh -c '...'` command string - see its doc comment.
                let mut c = Command::new("zsh");
                c.args(["-lc", NODE_BIN_RESOLVER_PREAMBLE]);
                c
            }
            (Interpreter::Node, false) => polling_ssh(host, NODE_BIN_RESOLVER_PREAMBLE),
        };
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }

    fn preamble(self) -> String {
        match self {
            Interpreter::Sh => CLAUDE_BIN_RESOLVER_PREAMBLE
                .replace("__CLAUDE_CALL_TIMEOUT__", &CLAUDE_CALL_TIMEOUT_SECS.to_string()),
            Interpreter::Node | Interpreter::BoundedSh(_) => String::new(),
        }
    }
}

/// A one-liner run in place of a bare `sh`/`ssh host sh` (local or remote respectively) that picks whichever self-timeout mechanism the shell has available and `exec`s into a bounded `sh` reading the actual script from the same stdin - mirroring the timeout/gtimeout/perl-alarm fallback triple already established in [`CLAUDE_BIN_RESOLVER_PREAMBLE`]'s `AKI_CLAUDE_TMO`, but bounding the ENTIRE invocation instead of one call inside it.
///
/// WHY: if the local side's own timeout fires first and kills its `ssh`/`sh` client, the process it was talking to does not reliably die with it (SIGHUP does not dependably reach a grandchild through a login shell - the same caveat `AKI_CLAUDE_TMO`'s doc comment already notes for `claude` itself). Left unbounded, that orphan can run indefinitely - holding memory on the remote host - with nothing left on the local side able to clean it up. Making the shell bound *itself* means it always exits on its own, independent of what happens to the connection driving it.
///
/// The final `else exec sh` (no timeout/gtimeout/perl found at all) is a real, if rare, residual gap - logged to stderr so it is visible rather than silent, same precedent as `AKI_CLAUDE_TMO`'s own last-resort branch.
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

/// Prepended to every `sh` script sent through [`run_interpreter_timeout`], local or remote.
/// Resolves a `claude` binary path into `$CLAUDE_BIN` via static, deterministic file checks
/// BEFORE falling back to PATH/login-shell lookup.
///
/// WHY: provision was seen failing with `exit=127 command not found: claude`
/// inside `zsh -lc`/`bash -lc`, seconds after this app's own cold start, then succeeding
/// again minutes later with the identical command - a PATH race against the user's shell
/// rc/profile (nvm, path_helper, etc.) not having finished sourcing yet at that exact
/// moment. A `[ -x "$path" ]` file-existence test has no dependency on rc-sourcing timing,
/// so trying known install locations first structurally removes the race instead of
/// patching each call site that happens to invoke `claude` today.
///
/// NOTE: mac-only path list for now - this app currently ships for macOS only (see
/// CLAUDE.md). If a Linux/Windows build ships later, extend the list below.
const CLAUDE_BIN_RESOLVER_PREAMBLE: &str = r#"
_resolve_claude_bin() {
    for _c in "$HOME/.local/bin/claude" "$HOME/.claude/local/claude" \
              /opt/homebrew/bin/claude /usr/local/bin/claude; do
        [ -x "$_c" ] && { printf '%s' "$_c"; return; }
    done
    command -v claude 2>/dev/null && return
    if command -v zsh >/dev/null 2>&1; then
        zsh -lc 'command -v claude' 2>/dev/null && return
    fi
    bash -lc 'command -v claude' 2>/dev/null
}
CLAUDE_BIN=$(_resolve_claude_bin)
[ -z "$CLAUDE_BIN" ] && CLAUDE_BIN=claude
export CLAUDE_BIN

# Prefix that bounds a single `claude` call ON THE REMOTE. Scripts must expand it directly into
# the command string (AKI_CLAUDE_TMO'$CLAUDE_BIN' ...) rather than wrap it in a shell function  - 
# these calls run inside `zsh -lc "..."`, a child shell that does not inherit functions.
#
# WHY this matters more than any cleanup: when the local side kills the SSH, the remote `claude`
# does NOT reliably die with it (SIGHUP does not dependably reach a grandchild through a login
# shell). A `claude` blocked on a stalled API call then runs forever, holding hundreds of MB.
# Bounding it here means it ends itself and there is nothing left to clean up.
#
# gtimeout is the Homebrew coreutils name on macOS, where `timeout` is not present by default.
# If neither exists we fall back to unbounded - same as before this fix, with the pkill sweep in
# agent_usage.rs as the only net. That gap is logged so it is visible rather than silent.
if command -v timeout >/dev/null 2>&1; then
    AKI_CLAUDE_TMO="timeout -k 5 __CLAUDE_CALL_TIMEOUT__ "
elif command -v gtimeout >/dev/null 2>&1; then
    AKI_CLAUDE_TMO="gtimeout -k 5 __CLAUDE_CALL_TIMEOUT__ "
elif command -v perl >/dev/null 2>&1; then
    # Stock macOS has neither timeout nor gtimeout (verified), so without this branch the
    # single most important host type for this app would silently keep the unbounded behavior
    # that caused the leak. perl ships with every macOS and virtually every Linux. `alarm` then
    # `exec` replaces the perl process with claude itself, so SIGALRM lands on claude directly  - 
    # no wrapper left holding a child, which is exactly the failure mode being fixed.
    AKI_CLAUDE_TMO="perl -e 'alarm shift; exec @ARGV or exit 127' __CLAUDE_CALL_TIMEOUT__ "
else
    AKI_CLAUDE_TMO=""
    printf '[SHELL:preamble] WARNING no timeout/gtimeout/perl on this host - claude calls run unbounded\n' >&2
fi
export AKI_CLAUDE_TMO
"#;

/// Resolves a `node` binary on the REMOTE host and `exec`s into it - used as the ssh *remote command itself* for [`Interpreter::Node`] (see `polling_ssh(host, NODE_BIN_RESOLVER_PREAMBLE)` in [`Interpreter::spawn`]), not prepended to stdin the way [`CLAUDE_BIN_RESOLVER_PREAMBLE`] is.
///
/// ALSO USED LOCALLY, verbatim: the local branch of [`Interpreter::Node`] runs `zsh -lc <this>` instead of the old bare `zsh -lc node`, which was the same cold-start PATH race on the local machine (stack-tauri A2). One constant, both paths - the candidate list must never be copied to a second site. The local outer shell stays a login shell purely so the resolver's last-resort `command -v node` still sees the user's rc-built PATH; correctness no longer depends on that shell having finished sourcing, because the `[ -x path ]` candidates are tried first.
///
/// WHY THE MECHANISM DIFFERS FROM CLAUDE_BIN_RESOLVER_PREAMBLE: that preamble is sh source text prepended ahead of a POSIX-sh SCRIPT sent over the same stdin stream the interpreter (`sh`) reads as its own program, so concatenating text ahead of more shell text is safe. Here the remote interpreter is `node`, and stdin carries pure JavaScript (`scripts/get-antigravity-usage.js`) - prepending shell source ahead of that would hand node a syntax error. Reading a shell preamble off the SAME stdin stream and then `exec`-ing into node partway through is *also* unsafe: a shell running a script fed over stdin (e.g. `ssh host sh`) commonly performs a large buffered read of that fd, so bytes belonging to the JS portion can already be sitting in the shell's own read buffer - unreachable once `exec` replaces the process image - and are silently lost (implementation-defined per shell; a real failure mode with dash-style buffering, not a theoretical one).
///
/// Baking the resolution into the *remote command* instead sidesteps this: `ssh host "<this>"` runs via the remote user's non-interactive shell (`$SHELL -c "<this>"`) as one process BEFORE any script bytes are consumed - this script never reads stdin at all (only `[ -x path ]` existence checks and `command -v`), so the stdin pipe is still completely untouched at the moment `exec "$NODE_BIN"` runs, and the resolved node binary receives the JS whole.
///
/// Candidates tested with `[ -x "$path" ]` FIRST (zero dependency on rc-sourcing timing, same cold-start race CLAUDE_BIN_RESOLVER_PREAMBLE's doc comment explains), falling back to `command -v node` only if none match.
///
/// NOTE: mac-only fixed paths (this app ships macOS-only, see CLAUDE.md) plus the nvm glob, which is host-OS-agnostic (nvm lays itself out the same way on macOS and Linux). Strictly POSIX sh - no bashisms, no `pipefail`, no `[[ ]]`, no arrays.
///
/// WHY IT IS WRAPPED IN `sh -c '...'` AND KEPT TO ONE LINE: `ssh host "<cmd>"` does NOT run `<cmd>` under `sh` - sshd runs it under the remote user's *login shell* (`$SHELL -c "<cmd>"`). A remote user whose shell is `fish` or `csh` would hit a syntax error on `[ -x ]`, `&&`, the `$(...)` substitution and the multi-line function body alike - failing non-zero, which `get_antigravity_usage` swallows as `Ok(None)`, i.e. re-creating the exact silent-forever failure this constant exists to fix, just with a different trigger. The bare `node` this replaced was immune to that because a bare command name parses in any shell. `sh -c '...'` restores that immunity: every shell (POSIX, csh, fish) parses a single-quoted single-line argument the same way, and the body then genuinely runs under `sh`. This is also why the script must contain NO single quote of its own - hence `NODE_BIN=$_c` rather than a `printf '%s'` - and why it is one line rather than a readable multi-line block.
///
/// `sh -c` also keeps the stdin-safety property intact: the script arrives via argv, never off stdin, so the JS on stdin is still completely unread when `exec "$NODE_BIN"` runs.
const NODE_BIN_RESOLVER_PREAMBLE: &str = r#"sh -c 'for _c in $HOME/.nvm/versions/node/*/bin/node "$HOME/.local/bin/node" /opt/homebrew/bin/node /usr/local/bin/node /usr/bin/node; do [ -x "$_c" ] && { NODE_BIN=$_c; break; }; done; [ -n "$NODE_BIN" ] || NODE_BIN=$(command -v node 2>/dev/null); [ -n "$NODE_BIN" ] || NODE_BIN=node; export NODE_BIN; exec "$NODE_BIN"'"#;

/// Kills the remote/local process if it overruns `timeout_secs`, returning an explicit timeout error instead of blocking forever. One funnel for every interpreter this app spawns a script through (SSoT - see stack-tauri rule's PATH-race preamble note: one funnel, not per-call-site patches) - [`Interpreter`] selects the local/remote invocation and preamble.
///
/// `host == "local"`/`"localhost"` runs `script` through the interpreter's local invocation instead of SSH - this is how usage is monitored when the agent runs on the same machine as this app, no remote involved.
///
/// Held for the whole call: [`host_lock`], so this and every other feature's calls to the same host serialize against each other (see that function's doc comment for why).
fn run_interpreter_timeout(
    host: &str,
    interpreter: Interpreter,
    script: &str,
    timeout_secs: u64,
) -> Result<Output, String> {
    let host_mutex = host_lock(host);
    let _host_guard = host_mutex.lock().unwrap_or_else(|e| e.into_inner());

    let local = is_local_host(host);
    let mut child = interpreter
        .spawn(host, local)
        .map_err(|e| format!("Failed to spawn {}: {}", if local { "local process" } else { "SSH" }, e))?;

    // Drain stdout/stderr on dedicated threads BEFORE writing stdin, so a large script can't deadlock against a full output pipe that ssh isn't draining yet.
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
        let preamble = interpreter.preamble();
        if !preamble.is_empty() {
            stdin
                .write_all(preamble.as_bytes())
                .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        }
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| format!("Failed to write to stdin: {}", e))?;
        // stdin dropped here → closes the pipe so the remote process sees EOF
    }

    // Poll for completion with a hard timeout; kill on overrun.
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

#[derive(Serialize)]
pub struct AgentUsageResponse {
    pub content: String,
    pub fetched_at: String,
    pub file_modified_at: String,
}

/// One answer to `get_agent_usage`, and the reason there is no reading when there isn't one.
///
/// WHY THIS EXISTS RATHER THAN A BARE `Option`. "The host refused the connection" and "the host
/// answered and had nothing to report" are completely different events that both used to arrive at
/// the frontend as the same `null`. The circuit breaker in `usageMonitor.js` counts the first and
/// must ignore the second, and with one shared `null` it could do neither: it reset its counter on
/// every `null`, so the exact incident it was written for (host refusing TCP → `ssh` exit 255) could
/// never trip it, while a legitimately quiet host (no cache file yet, AG IDE not running) would have
/// tripped it if the reset were simply removed. Reachability is a property of the CALL, not of the
/// data, so it travels beside the data instead of being inferred from its absence.
///
/// `data: None` with `host_answered: true` is the ordinary quiet poll and stays completely silent in
/// the UI - this type does not turn any previously-swallowed condition into a user-facing error.
#[derive(Serialize)]
pub struct AgentUsageResult {
    /// True when the host itself ran the probe and exited on its own, whatever it exited with.
    pub host_answered: bool,
    /// Why there is no reading. `None` when `data` is present.
    pub miss_reason: Option<String>,
    pub data: Option<AgentUsageResponse>,
}

impl AgentUsageResult {
    fn hit(data: AgentUsageResponse) -> Self {
        Self { host_answered: true, miss_reason: None, data: Some(data) }
    }

    /// The host answered; there is simply no reading this poll.
    fn miss(reason: impl Into<String>) -> Self {
        Self { host_answered: true, miss_reason: Some(reason.into()), data: None }
    }

    /// The call never reached the host (connection refused, DNS, auth, network unreachable).
    fn unreachable(reason: impl Into<String>) -> Self {
        Self { host_answered: false, miss_reason: Some(reason.into()), data: None }
    }
}

/// Whether the HOST itself answered, given the exit status of one probe.
///
/// `ssh` reserves exit status 255 for its own failures - connection refused, host key, DNS, auth,
/// network unreachable - and in every one of those cases the remote command never ran. Any other
/// status is the remote script's own exit code, i.e. the host was there. A local probe always
/// answers: there is no connection to fail.
///
/// Known edge: a remote script that itself exited 255 would be read as unreachable. Neither probe
/// this module ships does (they exit 0, 1 or 127), and the cost of a misread is one tick on the
/// frontend's failure counter, not a wrong number on screen.
fn host_answered(host: &str, exit_code: i32) -> bool {
    is_local_host(host) || exit_code != 255
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Truncate a string for safe log preview (no newlines, bounded length).
fn preview(s: &str, max: usize) -> String {
    let s = s.trim();
    let s = if s.len() > max {
        // Cut at a char boundary at/below `max` so multi-byte UTF-8 (e.g. Vietnamese session names in the cached JSON) never panics.
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    } else {
        s
    };
    s.replace('\n', "\u{21b5}").replace('\r', "")
}

/// Returns "CC" for claudecode, "AG" for antigravity, or the agent name as-is.
#[inline]
fn ab(agent: &str) -> &str {
    match agent {
        "claudecode"  => "CC",
        "antigravity" => "AG",
        other         => other,
    }
}

#[tauri::command]
pub async fn provision_agent_usage(agent_name: String, host: String) -> Result<bool, String> {
    // The host becomes an argv element for `ssh` a few frames down. `ssh` parses its own argv, so
    // a value like `-oProxyCommand=…` would run that command on THIS Mac - and this command is
    // reachable from the companion seam, not just the app's own UI. Same guard as
    // projects.rs::validate_project and git.rs.
    crate::system::validate_remote_host(&host)?;
    // run_remote_script (below) is fully synchronous (wait/poll loop, up to REMOTE_SCRIPT_TIMEOUT_SECS). Running it directly on the async executor starves a tokio worker for the same duration - spawn_blocking offloads to the blocking thread-pool, same pattern as get_agent_usage/logout_antigravity (P5, docs/research/claudecode-usage-FINAL.md; this pair was the one gap the stack-tauri never-block-the-UI audit had missed).
    tauri::async_runtime::spawn_blocking(move || provision_agent_usage_sync(&agent_name, &host))
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

fn provision_agent_usage_sync(agent_name: &str, host: &str) -> Result<bool, String> {
    logger::info("PROVISION", &format!("{} host={}", ab(agent_name), host));

    if agent_name != "claudecode" {
        logger::debug("PROVISION", &format!("skip {}", ab(agent_name)));
        return if agent_name == "antigravity" { Ok(true) } else { Err("Unknown agent".into()) };
    }

    const SCRIPT: &str = include_str!("../../scripts/provision-claudecode.sh");
    let output = run_remote_script(host, SCRIPT)?;
    let ok = output.status.success();
    logger::info("PROVISION", &format!("exit={} ok={}", output.status.code().unwrap_or(-1), ok));
    let err = String::from_utf8_lossy(&output.stderr);
    if !ok {
        let err_preview = preview(&err, 200);
        logger::error("PROVISION", &format!("stderr={}", err_preview));
        return Err(format!("Provision failed: {}", err));
    }
    // The script now always exits 0 (auth caching is best-effort), but a non-empty stderr still carries the [SHELL:provision] empty-auth diagnostic - a real signal correlated with Bug B (empty /usage). Log it at ERROR so it lands in usage.log even in production (no --debug).
    if !err.trim().is_empty() {
        logger::error("PROVISION", &format!("stderr (non-fatal)={}", preview(&err, 200)));
    }
    Ok(true)
}

#[tauri::command]
pub async fn get_agent_usage(
    agent_name: String,
    host: String,
) -> Result<AgentUsageResult, String> {
    // See provision_agent_usage: every boundary where a host becomes an ssh argv element validates.
    crate::system::validate_remote_host(&host)?;
    // Both inner fns are fully synchronous (wait_with_output, thread::sleep). Running them directly on the async executor starves it and freezes the UI. spawn_blocking offloads to the Tauri blocking thread-pool.
    tauri::async_runtime::spawn_blocking(move || {
        if agent_name == "claudecode" {
            return get_claudecode_usage(&host);
        }
        if agent_name == "antigravity" {
            return get_antigravity_usage(&host);
        }
        Err("Unknown agent".into())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Emit each non-empty shell stderr line at debug level.
fn log_shell_stderr(tag: &str, stderr: &str) {
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        logger::debug(tag, "stderr: (empty)");
        return;
    }
    logger::debug(tag, &format!("stderr: {} lines", lines.len()));
    for line in lines {
        logger::debug(tag, &format!("  | {}", line));
    }
}

/// True the first time it's called for a given host in this app process, false after  - used to force one bypass of the auth-cache TTL right after app launch (see `AKI_FORCE_AUTH_REFRESH` in get-claudecode-usage.sh). A CC account switch is rare and happens outside the app, so there's no reliable in-app event to hook; "app was just opened" is the one moment a stale cached email is most likely to be noticed and easiest to guarantee correct, without adding any extra polling.
fn cc_auth_force_needed(host: &str) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    seen.lock().unwrap().insert(host.to_string())
}

/// True the first time it's called for a given host in this app process, false after - used to promote the AG remote-probe's otherwise-silent Ok(None) swallow (below) to a visible ERROR log line exactly once per host per session when the failure looks like a PATH-resolution miss (exit 127 / "command not found"), instead of leaving a *permanent* condition indistinguishable from the many genuinely transient soft-misses this function also swallows (IDE not running, mid-restart, signed out, one slow RPC). That indistinguishability is exactly what made the original NODE_BIN PATH bug invisible for a full release: every poll logged the identical "soft-miss" debug line whether the cause was permanent or transient, and debug-level never surfaces without --debug. This does not change the Ok(None) return or add any UI element (Extreme Narrow) - it only makes a first-occurrence-per-host case reachable in usage.log without --debug, then falls back to the existing debug-level line on every subsequent tick so a genuinely down host does not spam ERROR forever.
fn ag_node_missing_once(host: &str) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    seen.lock().unwrap().insert(host.to_string())
}

fn get_claudecode_usage(host: &str) -> Result<AgentUsageResult, String> {
    logger::debug("GET_USAGE", &format!("start host={}", host));

    const SCRIPT: &str = include_str!("../../scripts/get-claudecode-usage.sh");
    let force_auth = cc_auth_force_needed(host);
    let script_owned;
    let script: &str = if force_auth {
        logger::info("GET_USAGE", "first check this session - forcing auth refresh (bypass cache TTL)");
        script_owned = format!("AKI_FORCE_AUTH_REFRESH=1\n{}", SCRIPT);
        &script_owned
    } else {
        SCRIPT
    };
    // Transport failure (spawn failed, or the bounded run timed out) resolves to `unreachable`,
    // not an IPC `Err`. Propagating it with `?` made CC and AG behave differently through the same
    // funnel for the identical condition: AG's arm returns `unreachable(...)` and the card keeps
    // showing its last cached reading, while CC's `?` painted a full-card error banner on every
    // poll tick - the flickering-banner instability this funnel exists to end (see the comments in
    // get_antigravity_usage). `unreachable` still lets the frontend's breaker count the failure,
    // which is the part that actually needs to be visible.
    let output = match run_remote_script(host, script) {
        Ok(o) => o,
        Err(e) => {
            logger::debug("GET_USAGE", &format!("soft-miss (spawn/timeout): {}", e));
            return Ok(AgentUsageResult::unreachable(e));
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    logger::debug("GET_USAGE", &format!(
        "exit={} stdout_b={} stderr_b={}",
        exit_code, stdout.len(), stderr.len()
    ));

    log_shell_stderr("GET_USAGE", &stderr);

    if !output.status.success() {
        // The one branch where "no reading" may mean the host was never there: an `ssh` that could
        // not connect exits 255 having run nothing. Reported as unreachable so the frontend's
        // breaker can see it - this used to be the same silent `null` as a quiet host, which is why
        // a host refusing TCP could be probed at full rate for 24 minutes (docs/plan §3.8).
        if !host_answered(host, exit_code) {
            logger::error("GET_USAGE", &format!("host unreachable (ssh exit={})", exit_code));
            return Ok(AgentUsageResult::unreachable(format!("ssh could not reach {} (exit 255)", host)));
        }
        logger::error("GET_USAGE", &format!("shell exit={}", exit_code));
        return Ok(AgentUsageResult::miss(format!("probe exited {}", exit_code)));
    }

    if stdout.trim().is_empty() {
        logger::info("GET_USAGE", "null: no cache");
        return Ok(AgentUsageResult::miss("no cache"));
    }

    // STALE_RESET signal
    if stdout.trim() == "|||STALE_RESET|||" {
        logger::info("GET_USAGE", "null: STALE_RESET");
        return Ok(AgentUsageResult::miss("STALE_RESET"));
    }

    logger::debug("GET_USAGE", &format!("stdout: {}", preview(&stdout, 300)));

    // ── Parse delimiter chain ───────────────────────────────────────────── Expected: <json>|||MTIME|||<ts>|||SUBTYPE|||<st>|||TIER|||<tier>|||AUTHINFO|||<json>

    let parts: Vec<&str> = stdout.split("|||MTIME|||").collect();
    logger::debug("GET_USAGE", &format!("mtime_parts={}", parts.len()));
    if parts.len() != 2 {
        logger::error("GET_USAGE", "no MTIME delimiter");
        return Ok(AgentUsageResult::miss("malformed probe output (no MTIME delimiter)"));
    }

    let content_raw = parts[0].trim();
    let after_mtime = parts[1];

    let mtime_split: Vec<&str> = after_mtime.split("|||SUBTYPE|||").collect();
    let mtime_sec = mtime_split[0].trim().parse::<i64>().unwrap_or(0);
    logger::debug("GET_USAGE", &format!("mtime={} subtype_parts={}", mtime_sec, mtime_split.len()));

    let (sub_type, tier, auth_json) = if mtime_split.len() > 1 {
        let sub_split: Vec<&str> = mtime_split[1].split("|||TIER|||").collect();
        let st = sub_split[0].trim();
        logger::debug("GET_USAGE", &format!("subtype='{}' tier_parts={}", st, sub_split.len()));
        let (t, auth) = if sub_split.len() > 1 {
            let tier_split: Vec<&str> = sub_split[1].split("|||AUTHINFO|||").collect();
            let tier_val = tier_split[0].trim();
            let auth_val = if tier_split.len() > 1 { tier_split[1].trim() } else { "{}" };
            logger::debug("GET_USAGE", &format!("tier='{}' authinfo_b={}", tier_val, auth_val.len()));
            (tier_val, auth_val)
        } else {
            logger::debug("GET_USAGE", "no TIER delimiter");
            ("Unknown", "{}")
        };
        (st, t, auth)
    } else {
        logger::debug("GET_USAGE", "no SUBTYPE delimiter");
        ("Unknown", "Unknown", "{}")
    };

    // ── JSON parse of cache content ───────────────────────────────────────
    let content_len = content_raw.len();
    let mut v: serde_json::Value = match serde_json::from_str(content_raw) {
        Ok(val) => {
            logger::debug("GET_USAGE", &format!("json_ok b={}", content_len));
            val
        }
        Err(e) => {
            // A miss, NOT an empty hit. Substituting `{}` here and falling through to
            // `AgentUsageResult::hit` below reported a successful reading with no rate limits in
            // it: the frontend overwrote its last good data with nothing, drew empty bars, and
            // marked them fresh (stale=false) - i.e. corrupt output looked exactly like "you have
            // used 0%". The sibling delimiter failure a few frames up already returns a miss for
            // the same class of problem; this branch was the asymmetry.
            logger::error("GET_USAGE", &format!("json_parse err={} b={}", e, content_len));
            return Ok(AgentUsageResult::miss("malformed probe output (cache JSON did not parse)"));
        }
    };

    // ── Inject metadata ───────────────────────────────────────────────────
    if let Some(obj) = v.as_object_mut() {
        if sub_type != "Unknown" {
            obj.insert("subscriptionType".to_string(), serde_json::json!(sub_type));
        }
        if tier != "Unknown" {
            obj.insert("rateLimitTier".to_string(), serde_json::json!(tier));
        }
        match serde_json::from_str::<serde_json::Value>(auth_json) {
            Ok(auth) => {
                let email = auth.get("email").and_then(|v| v.as_str()).unwrap_or("");
                let org   = auth.get("orgName").and_then(|v| v.as_str()).unwrap_or("");
                logger::debug("GET_USAGE", &format!("auth email='{}' org='{}'", email, org));
                if !email.is_empty() { obj.insert("email".to_string(), serde_json::json!(email)); }
                if !org.is_empty()   { obj.insert("orgName".to_string(), serde_json::json!(org)); }
            }
            Err(e) => {
                logger::error("GET_USAGE", &format!("auth_parse err={} preview={}", e, preview(auth_json, 100)));
            }
        }
    }

    // ── Rate limits summary ───────────────────────────────────────────────
    if let Some(obj) = v.as_object() {
        let now = now_secs();
        // `rate_limits` is an open map (five_hour, seven_day, and model-scoped weeklies such as
        // seven_day_opus/_sonnet/_oauth_apps that Anthropic adds without notice) - enumerate
        // whatever arrived instead of naming two keys, or a new bucket is invisible in the log
        // while the UI is already drawing it.
        let summary = obj.get("rate_limits")
            .and_then(|r| r.as_object())
            .map(|rl| {
                if rl.is_empty() { return "EMPTY".to_string(); }
                let mut keys: Vec<&String> = rl.keys().collect();
                keys.sort();
                keys.iter()
                    .map(|k| {
                        let b = &rl[*k];
                        let pct    = b.get("used_percentage").and_then(|v| v.as_i64()).unwrap_or(-1);
                        let resets = b.get("resets_at").and_then(|v| v.as_i64()).unwrap_or(0);
                        let state  = if resets == 0 { "no_reset" }
                                     else if now - resets > 0 { "PAST" } else { "future" };
                        format!("{}=[pct={} resets_at={} overdue_s={} state={}]",
                                k, pct, resets, now - resets, state)
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| "MISSING".to_string());
        logger::debug("GET_USAGE", &format!("rl {}", summary));
    }

    let content = serde_json::to_string(&v).unwrap_or_default();
    logger::debug("GET_USAGE", &format!("done mtime={} b={}", mtime_sec, content.len()));
    Ok(AgentUsageResult::hit(AgentUsageResponse {
        content,
        fetched_at: now_secs().to_string(),
        file_modified_at: mtime_sec.to_string(),
    }))
}

fn get_antigravity_usage(host: &str) -> Result<AgentUsageResult, String> {
    logger::debug("USAGE:antigravity", &format!("start host={}", host));

    let script = include_str!("../../scripts/get-antigravity-usage.js");

    // P2 (docs/research/claudecode-usage-FINAL.md): this used to spawn+wait_with_output() with NO timeout - a blackholed SSH/local probe wedged `isChecking` permanently on the JS side, freezing every subsequent poll tick for this source. Routed through the same bounded funnel as CC (run_interpreter_timeout / Interpreter::Node) so it always resolves within REMOTE_SCRIPT_TIMEOUT_SECS. A timeout is swallowed to Ok(None) - same "transient monitor condition" policy as the non-zero-exit branch below, so it reads as one more silent poll-miss instead of a new flickering error state that didn't exist before this fix.
    let output = match run_remote_node_timeout(host, script) {
        Ok(o) => o,
        Err(e) => {
            // Still swallowed rather than raised as an IPC `Err` (that flickering banner every poll
            // WAS the usage instability this funnel was built to end), but no longer indistinguishable
            // from a quiet host: a spawn failure or a 30s timeout is the host not answering, and the
            // breaker is entitled to count it.
            logger::debug("USAGE:antigravity", &format!("soft-miss (spawn/timeout): {}", e));
            return Ok(AgentUsageResult::unreachable(e));
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    logger::debug("USAGE:antigravity", &format!(
        "exit={} stdout_b={} stderr_b={}",
        exit_code, output.stdout.len(), output.stderr.len()
    ));

    if !output.status.success() {
        // Every non-zero exit here is a *transient monitor* condition, never a user-facing fault: the IDE isn't running, is mid-restart, hasn't opened its Connect port yet, was just signed out, or a single localhost RPC probe timed out. To the UI they all mean the same thing - "no live reading this poll" - and the frontend already handles that (composable null path shows the last cached account). Surfacing any of them as an IPC Err only produced a flickering error banner every poll: that WAS the usage instability. So swallow all AG script failures to Ok(None); just log the reason.
        let stderr = String::from_utf8_lossy(&output.stderr);
        // A PATH-resolution miss (exit 127, or the shell's own "command not found" text) is a PERMANENT condition on a given host, not a transient one - and unlike the other cases this branch swallows (IDE mid-restart, signed out, etc.), simply retrying on the next poll tick will never fix it. Surface it once per host at ERROR (visible without --debug) so it doesn't silently repeat unnoticed for a whole release the way the original NODE_BIN bug did; every other tick, and every other failure shape, keeps the existing debug-level line - the Ok(None) contract to the frontend is unchanged either way (see docs/arch/usage-antigravity.md's swallow-to-Ok(None) rationale).
        let looks_like_path_miss = exit_code == 127 || stderr.to_lowercase().contains("command not found");
        if looks_like_path_miss && ag_node_missing_once(host) {
            logger::error("USAGE:antigravity", &format!(
                "node not found host={} exit={} stderr={} - see NODE_BIN_RESOLVER_PREAMBLE in agent_usage.rs",
                host, exit_code, preview(&stderr, 200)
            ));
        } else {
            logger::debug("USAGE:antigravity", &format!("soft-miss: {}", stderr.trim()));
        }
        // Exit 127 is the host answering with "no node here" - permanent, but not a reachability
        // failure, and halting the poll loop would not help. Only ssh's own 255 counts as unreachable.
        if !host_answered(host, exit_code) {
            return Ok(AgentUsageResult::unreachable(format!("ssh could not reach {} (exit 255)", host)));
        }
        return Ok(AgentUsageResult::miss(format!("probe exited {}", exit_code)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        logger::debug("USAGE:antigravity", "done: null empty stdout");
        return Ok(AgentUsageResult::miss("no live AG session"));
    }

    let now = now_secs().to_string();
    logger::debug("USAGE:antigravity", &format!("done: ok b={}", stdout.len()));
    Ok(AgentUsageResult::hit(AgentUsageResponse {
        content: stdout.to_string(),
        fetched_at: now.clone(),
        file_modified_at: now,
    }))
}

/// Must match the actual /Applications/*.app bundle name - used for `osascript quit app`, `pkill`, the Application Support folder name, and the "<name> Safe Storage" Keychain item.
const ANTIGRAVITY_APP_NAME: &str = "Antigravity IDE";

/// Electron userData files that hold only the logged-in web session (cookies, chromium local/session storage, network identity state) - deleting these is equivalent to a browser "sign out", while leaving User/ (settings, keybindings, snippets, extensions, workspaceStorage) and globalStorage/ (extension state incl. rules/permissions) untouched.
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

/// globalState keys that hold the live Antigravity OAuth session. Deleting these forces a real re-login while leaving every other globalState row (settings, extension state) intact.
const ANTIGRAVITY_AUTH_KEYS: &[&str] = &[
    "antigravityUnifiedStateSync.oauthToken",
    "antigravityUnifiedStateSync.userStatus",
];

/// Delete the OAuth session rows from `User/globalStorage/state.vscdb` (and `.backup`) via the system `sqlite3`. Best-effort: any failure (no sqlite3, file absent) is a silent no-op so a partial logout still wipes cookies + Keychain. Must be called only after the IDE is quit.
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
        // Quit the app first so Chromium isn't holding these files open while we delete them.
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

        // THE actual credential. Antigravity keeps its live OAuth session in VS Code's globalState SQLite store (User/globalStorage/state.vscdb) under the keys `antigravityUnifiedStateSync.oauthToken` / `.userStatus`. These are NOT Electron safeStorage ciphertext (they carry no v10/v11 prefix), so wiping cookies and the Keychain "Safe Storage" key above does NOT invalidate them - the IDE re-reads the token verbatim on next launch and silently signs back in. That was the "logout does nothing" bug. We must delete these two rows from state.vscdb (and its .backup, which Antigravity restores from if the primary is missing). The app is already quit above, so the SQLite file is unlocked. macOS ships /usr/bin/sqlite3; deleting only these two keys leaves all other globalState (settings, extension state, rules) untouched.
        remove_antigravity_auth_rows(&base);

        // The actual OAuth session survives a plain file wipe: Electron's `safeStorage` encrypts it and stores only the ciphertext in app files (state.vscdb etc.), while the AES key itself lives in exactly one macOS Keychain item named "<AppName> Safe Storage". Deleting that single, precisely-named item - not a keychain scan/dump - makes the stored ciphertext permanently undecryptable, which is what actually forces re-login, without touching User/ or globalStorage/ (so extensions, settings, rules, and permissions all survive untouched).
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The distinction the frontend circuit breaker is built on (docs/plan/done/1.20.1-flow-audit-fixes.md
    /// §3.8): only `ssh`'s own 255 means the host never answered.
    #[test]
    fn ssh_255_is_the_only_unreachable_signal() {
        assert!(!host_answered("hostB", 255), "ssh 255 = never reached the host");
        assert!(host_answered("hostB", 0));
        assert!(host_answered("hostB", 1), "remote script's own failure - the host answered");
        assert!(host_answered("hostB", 127), "node missing on the remote - the host answered");
        assert!(host_answered("hostB", -1), "no code (signalled) - not a connection failure");
    }

    #[test]
    fn a_local_probe_always_answers() {
        assert!(host_answered("local", 255));
        assert!(host_answered("localhost", 255));
    }

    #[test]
    fn result_constructors_carry_reachability() {
        let miss = AgentUsageResult::miss("no cache");
        assert!(miss.host_answered && miss.data.is_none());
        let down = AgentUsageResult::unreachable("ssh could not reach hostB");
        assert!(!down.host_answered && down.data.is_none());
        let hit = AgentUsageResult::hit(AgentUsageResponse {
            content: "{}".into(),
            fetched_at: "0".into(),
            file_modified_at: "0".into(),
        });
        assert!(hit.host_answered && hit.miss_reason.is_none() && hit.data.is_some());
    }
}
