//! Sync conflict classification (docs/plan/done/conflict-detection-and-agy-report.md §2-3).
//!
//! Pure logic only - no rsync/SSH/filesystem I/O. `sync.rs` gathers the (L, R, B, sizes) inputs (a local
//! `fs::metadata` walk, the existing push dry-run, and the new `--out-format` pull dry-run) and calls this
//! module to decide what each differing file means, exactly the same split `sync.rs`'s existing
//! `classify_sync_counts` already uses for the baseline-only reclassification this module extends.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// rsync's own `--modify-window`, reused here so a mtime "equal" in this module means the same thing it
/// means to the rsync invocation that produced the file list (sync.rs's existing `--modify-window=2`).
pub const MODIFY_WINDOW_SECS: i64 = 2;

/// Above this many same-size files, the checksum residue (§3) stops calling `rsync -c` and marks the rest
/// conflict-unverified instead - a starting cap, per the plan's own wording.
pub const CHECKSUM_CAP: usize = 200;

fn mtimes_equal(a: u64, b: u64) -> bool {
    (a as i64 - b as i64).abs() <= MODIFY_WINDOW_SECS
}

fn mtime_gt(a: u64, b: u64) -> bool {
    a as i64 - b as i64 > MODIFY_WINDOW_SECS
}

fn mtime_lt(a: u64, b: u64) -> bool {
    b as i64 - a as i64 > MODIFY_WINDOW_SECS
}

/// One differing file's (L, R, B, sizes) - docs/plan/done/conflict-detection-and-agy-report.md §2's table
/// columns, plus the `.git` carve-out and the no-ancestor carve-out.
#[derive(Debug, Clone, Default)]
pub struct ClassifyInput {
    pub is_git: bool,
    /// A baseline exists for this (project, host) pair AND it was recorded against the target's CURRENT
    /// `remote_path` - callers pass `false` whenever `sync.rs::baseline_for_target` already returned `None`
    /// for that reason, so this module never re-derives the remote_path check itself.
    pub has_baseline: bool,
    pub baseline_mtime: Option<u64>,
    /// `None` = file does not exist locally at all.
    pub local_mtime: Option<u64>,
    /// `None` = no remote metadata for this path (the out-format call degraded, or this path never
    /// appeared in the pull-side diff at all).
    pub remote_mtime: Option<u64>,
    pub local_size: Option<u64>,
    pub remote_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    Push,
    /// Same push direction, but specifically because the remote regressed relative to the common ancestor
    /// (R < B - a stale/behind remote, docs/plan/done/conflict-detection-and-agy-report.md's evidence case).
    /// Counted into `push_count` exactly like `Push`; kept separate so the caller can surface the
    /// "remote is behind" signal (§4/§5) instead of that reason being invisible inside a plain push count.
    PushStaleRemote,
    Pull,
    /// Never a conflict; counted in its own `.git` group for the tooltip breakdown, but still assigned to
    /// push or pull by whichever direction's raw file list actually listed it (§2, §4).
    GitGroup,
    /// "Today's -u count" - not a conflict, no ancestor to compare against.
    Unclassified,
    Conflict,
    /// Same size on both sides after both changed since the baseline - needs the checksum residue call (§3).
    NeedsChecksum,
    /// Remote deleted the file and the local copy was never edited since the baseline - restores
    /// `sync.rs::classify_sync_counts`'s PUSH-side suppression (docs/feat/sync-flow.md §2's "Remote deleted
    /// X -> suppress from push_count" row) inside this classifier. Dropped from every count, never pushed
    /// and never a conflict.
    Suppressed,
}

/// Pure classifier over (L, R, B, sizes) - one match arm per row of
/// docs/plan/done/conflict-detection-and-agy-report.md §2's table, in the same order as the table itself.
pub fn classify_file(input: &ClassifyInput) -> FileClass {
    if input.is_git {
        return FileClass::GitGroup;
    }
    if input.local_mtime.is_none() {
        // Existing rule (sync.rs::classify_sync_counts): in baseline -> local deleted -> push; not in baseline -> remote created -> pull.
        return if input.baseline_mtime.is_some() {
            FileClass::Push
        } else {
            FileClass::Pull
        };
    }
    let l = input.local_mtime.unwrap();

    let b = match (input.has_baseline, input.baseline_mtime) {
        (true, Some(b)) => b,
        // No baseline for this host/target, or the file is not in the baseline while both sides are present: no ancestor to attribute the difference to.
        _ => return FileClass::Unclassified,
    };

    let r = match input.remote_mtime {
        Some(r) => r,
        // Remote lacks the file entirely (the pull-side metadata diff never listed it) - the same case
        // `classify_sync_counts`'s PUSH suppression table handles: unedited locally since the baseline means
        // the remote deleted it (suppress), edited locally since the baseline means a real push.
        None => {
            return if mtimes_equal(l, b) {
                FileClass::Suppressed
            } else {
                FileClass::Push
            };
        }
    };

    if mtimes_equal(l, b) && mtime_gt(r, b) {
        return FileClass::Pull;
    }
    if mtime_gt(l, b) && mtimes_equal(r, b) {
        return FileClass::Push;
    }
    if mtime_lt(r, b) {
        return FileClass::PushStaleRemote; // remote regressed relative to the common ancestor - stale.
    }
    if mtime_gt(l, b) && mtime_gt(r, b) {
        return match (input.local_size, input.remote_size) {
            (Some(ls), Some(rs)) if ls != rs => FileClass::Conflict,
            (Some(_), Some(_)) => FileClass::NeedsChecksum,
            // Size unavailable on one side - degrade rather than guess a class.
            _ => FileClass::Unclassified,
        };
    }
    // Anything else (e.g. L < B with R also <= B): not a shape the table assigns, today's -u count.
    FileClass::Unclassified
}

/// One file from `rsync --out-format='%n\t%l\t%M'` (the pull dry-run's remote metadata,
/// docs/plan/done/conflict-detection-and-agy-report.md § Execution steps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFileMeta {
    pub path: String,
    pub size: u64,
    pub mtime: u64,
}

/// rsync status/summary lines that are never file entries - the same list `sync.rs::rsync_change_files`
/// already filters, kept in sync deliberately since both read the same rsync stdout shape.
const RSYNC_STATUS_PREFIXES: &[&str] = &[
    "deleting ",
    "sending ",
    "receiving ",
    "sent ",
    "received ",
    "total size",
    "Number of",
    "building file list",
    "Transfer starting:",
    "Skip newer ",
];

/// Parses one `path\tsize\tYYYY/MM/DD-HH:MM:SS` line. `None` on anything that does not match that exact
/// shape - callers read a `None` on a line that survived the status-line filter as "this rsync's
/// `--out-format` did not produce what we expect" (the plan's degrade path: unknown rsync, or `%M` missing),
/// never as "a zero-size file at the epoch". `offset_secs_at` returns this machine's own UTC offset (seconds,
/// positive east of UTC) for a given UTC-ish instant - `%M` is printed by whichever rsync process runs it, in
/// that process's local time (rsync's `timestring()` calls `localtime()`), and the process running the
/// dry-run is always local (sync.rs's pull dry-run runs rsync as the local client over ssh), so this
/// machine's own offset is the correct one to undo it with, regardless of which side (local/remote) the file
/// itself lives on. Evaluated per-line rather than once (DST-safe): a single offset borrowed
/// from "now" is wrong for any file whose real mtime falls on the other side of a DST transition.
pub fn parse_out_format_line(line: &str, offset_secs_at: &dyn Fn(i64) -> i64) -> Option<RemoteFileMeta> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() != 3 {
        return None;
    }
    let path = parts[0].trim();
    if path.is_empty() || path.ends_with('/') {
        return None; // a directory entry, not a file
    }
    let size: u64 = parts[1].trim().parse().ok()?;
    let mtime = parse_rsync_mtime(parts[2].trim(), offset_secs_at)?;
    Some(RemoteFileMeta { path: path.to_string(), size, mtime })
}

/// `YYYY/MM/DD-HH:MM:SS`, rsync's own `%M` format - naive (no timezone field exists in this format at all),
/// interpreted as local wall-clock time and converted to a UTC unix timestamp by subtracting the offset
/// valid AT THAT INSTANT, not at "now" (DST-safe): the wall-clock digits are first read as if they were UTC
/// (`naive_local`) to get a same-day approximation - off by at most this zone's own offset, never enough to
/// cross into a different DST period than the real instant - and `offset_secs_at(naive_local)` is queried
/// with that approximation to pick the correct offset for the real date. `offset_secs_at` is a parameter
/// rather than read from the process environment here so this function stays pure and parallel-test-safe
/// (`agent.C5`'s scratchpad note re: process-global TZ mutation) - `local_utc_offset_secs_at` below is the
/// one real caller. Whether the Mac's stock openrsync emits this exact date shape is unverified here - see
/// docs/plan/done/conflict-detection-and-agy-report.md § Mac checks.
fn parse_rsync_mtime(s: &str, offset_secs_at: &dyn Fn(i64) -> i64) -> Option<u64> {
    let (date, time) = s.split_once('-')?;
    let mut d = date.split('/');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    if d.next().is_some() {
        return None;
    }
    let mut t = time.split(':');
    let hour: u32 = t.next()?.parse().ok()?;
    let min: u32 = t.next()?.parse().ok()?;
    let sec: u32 = t.next()?.parse().ok()?;
    if t.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let days = days_since_epoch(year, month, day)?;
    let naive_local = days * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    let utc = naive_local - offset_secs_at(naive_local);
    if utc < 0 {
        return None; // this machine's offset pushed a valid local time before the epoch - reject rather than wrap.
    }
    Some(utc as u64)
}

/// Days between 1970-01-01 and the given Gregorian date (Howard Hinnant's `days_from_civil`, public-domain
/// integer math - no date/time crate dependency added for one conversion).
fn days_since_epoch(year: i64, month: u32, day: u32) -> Option<i64> {
    if year < 1970 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// Result of parsing a whole `--out-format` pull dry-run's stdout.
#[derive(Debug, Clone, Default)]
pub struct ParsedRemoteDiff {
    pub files: Vec<RemoteFileMeta>,
    /// A line survived the status-line filter (so it looked like a file entry) but did not parse as
    /// `path\tsize\tmtime` - the unknown-format / missing-`%M` degrade path
    /// (docs/plan/done/conflict-detection-and-agy-report.md § Execution steps).
    pub degraded: bool,
}

pub fn parse_out_format_output(stdout: &str, offset_secs_at: &dyn Fn(i64) -> i64) -> ParsedRemoteDiff {
    let mut files = Vec::new();
    let mut degraded = false;
    for line in stdout.lines() {
        let l = line.trim();
        if l.is_empty() || RSYNC_STATUS_PREFIXES.iter().any(|p| l.starts_with(p)) {
            continue;
        }
        // Split first so a directory entry (path column ends with `/`) can be skipped as VALID before
        // `parse_out_format_line`'s stricter "None = degrade" contract would otherwise misclassify it -
        // `parse_out_format_line` itself also returns `None` for a directory (its own single-line contract
        // for a caller that has no separate "skip" outcome), so it is deliberately not reused here.
        let parts: Vec<&str> = l.split('\t').collect();
        if parts.len() == 3 && parts[0].trim().ends_with('/') {
            continue; // a directory entry - valid, not a parse failure
        }
        match parse_out_format_line(l, offset_secs_at) {
            Some(meta) => files.push(meta),
            None => degraded = true,
        }
    }
    ParsedRemoteDiff { files, degraded }
}

/// This machine's own UTC offset in seconds (positive east of UTC) valid AT `unix_ts_guess`, via the OS's
/// `tm_gmtoff` field for that instant - so a file whose mtime falls on the other side of a DST transition
/// from "now" still gets its own period's offset, not the offset in effect when the poll
/// happened to run. The one real (impure) caller of `parse_rsync_mtime`'s injected offset function. Never
/// call this from a test; inject a literal closure instead (see the module tests) so parallel test runs
/// never depend on, or race on, this process's actual TZ.
pub fn local_utc_offset_secs_at(unix_ts_guess: i64) -> i64 {
    #[cfg(unix)]
    unsafe {
        let ts = unix_ts_guess as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&ts, &mut tm);
        tm.tm_gmtoff as i64
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// Recreates the old pull-side `-u` meaning now that the pull dry-run itself drops `-u` to get remote
/// size/mtime for every differing file, not just the remote-newer ones
/// (docs/plan/done/conflict-detection-and-agy-report.md § Execution steps: "reapply the -u filter in Rust").
/// `local_mtimes` maps every candidate path to its local mtime, or `None` when the file is absent locally.
pub fn select_pull_after_u_filter<'a>(
    metas: &'a [RemoteFileMeta],
    local_mtimes: &HashMap<String, Option<u64>>,
) -> Vec<&'a RemoteFileMeta> {
    metas
        .iter()
        .filter(|m| match local_mtimes.get(&m.path) {
            // rsync `-u` skips a file only when the RECEIVER (local, for a pull) is strictly newer than the
            // sender - it does NOT require the sender to be strictly newer. An equal-mtime, different-size
            // pair (already known to differ, since it is in `metas` at all) is still pulled under real `-u`
            // semantics; only "local is newer beyond the modify-window" is excluded.
            Some(Some(local)) => !mtime_gt(*local, m.mtime),
            Some(None) | None => true, // absent locally - remote-created, always a pull candidate
        })
        .collect()
}

/// Splits the same-size residue (§3): only the first `cap` files get a real checksum call; the rest are
/// marked conflict-unverified instead of silently dropped or silently trusted.
pub fn split_checksum_residue(residue: Vec<String>, cap: usize) -> (Vec<String>, Vec<String>) {
    if residue.len() <= cap {
        (residue, Vec::new())
    } else {
        let mut checked = residue;
        let unverified = checked.split_off(cap);
        (checked, unverified)
    }
}

/// After the real `rsync -c --dry-run --files-from=<checked>` call: a file rsync still lists as differing
/// has different content despite equal size (a real conflict); a file it does NOT list converged (identical
/// content, e.g. the same commit pulled via git on both sides) and drops out of every count.
pub fn classify_checksum_result(
    checked: &[String],
    differing: &std::collections::HashSet<String>,
) -> (Vec<String>, Vec<String>) {
    let mut converged = Vec::new();
    let mut conflicts = Vec::new();
    for f in checked {
        if differing.contains(f) {
            conflicts.push(f.clone());
        } else {
            converged.push(f.clone());
        }
    }
    (converged, conflicts)
}

/// One conflict row returned to the frontend (`SyncStatusResult.conflicts`) and sent to `agy` on Explain.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConflictEntry {
    pub path: String,
    #[serde(default)]
    pub local_mtime: u64,
    #[serde(default)]
    pub remote_mtime: u64,
    #[serde(default)]
    pub local_size: u64,
    #[serde(default)]
    pub remote_size: u64,
    /// `false` for a same-size pair above `CHECKSUM_CAP` that was never actually checksummed.
    #[serde(default = "default_true")]
    pub verified: bool,
}

fn default_true() -> bool {
    true
}

/// Resolves the `agy` binary via static, well-known install-directory candidates first - the same
/// cold-start PATH-race pattern `agent_usage/claudecode.rs`'s `CLAUDE_BIN_RESOLVER_PREAMBLE` uses for
/// `claude` (`stack-tauri.A2`), applied to `agy` instead. Pure over an injectable existence check so the
/// candidate ORDER is unit-testable without touching the real filesystem; the real filesystem check and the
/// `command -v` PATH fallback are the caller's job (sync.rs's thin I/O wrapper).
pub fn resolve_agy_bin_with(home: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let candidates = [
        format!("{}/.local/bin/agy", home),
        format!("{}/.claude/local/agy", home),
        "/opt/homebrew/bin/agy".to_string(),
        "/usr/local/bin/agy".to_string(),
    ];
    candidates.into_iter().find(|c| exists(c))
}

/// Validates a conflict `path` arriving over IPC (Explain's `status.conflicts`, frontend-controlled) before
/// it is ever joined into a temp-dir path or a local file read (never trust an IPC `rel` to already
/// be relative, `..`-free, and control-character-free just because it originated from this app's own last
/// `check_sync_status` result - the IPC boundary is untrusted regardless of the usual origin, `coding.C4`).
pub fn validate_conflict_rel_path(rel: &str) -> Result<(), String> {
    if rel.is_empty() {
        return Err("empty conflict path".to_string());
    }
    if rel.starts_with('/') || rel.starts_with('~') {
        return Err(format!("conflict path '{}' must be relative", rel));
    }
    if rel.split('/').any(|seg| seg == "..") {
        return Err(format!("conflict path '{}' contains '..'", rel));
    }
    if rel.chars().any(|c| c.is_control()) {
        return Err(format!("conflict path '{}' contains control characters", rel));
    }
    Ok(())
}

/// Minimal LCS-based unified-diff-style text between two excerpts (docs/plan/done/conflict-detection-and-agy-report.md
/// §5: "unified diff of local vs remote"). Not byte-exact GNU diff output (no hunk headers, no surrounding
/// context window) - a compact, readable text for `agy` to narrate a conflict from, not a patch meant to be
/// applied. No external diff crate added for this one call site (`pattern.A2`: Rule of Three).
pub fn unified_diff(local: &str, remote: &str) -> String {
    let a: Vec<&str> = local.lines().collect();
    let b: Vec<&str> = remote.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push(format!("  {}", a[i]));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(format!("- {}", a[i]));
            i += 1;
        } else {
            out.push(format!("+ {}", b[j]));
            j += 1;
        }
    }
    while i < n {
        out.push(format!("- {}", a[i]));
        i += 1;
    }
    while j < m {
        out.push(format!("+ {}", b[j]));
        j += 1;
    }
    out.join("\n")
}

/// A filename that must never have its content sent to `agy` on Explain
/// (docs/plan/done/conflict-detection-and-agy-report.md § agy explanation: "binaries and secret-named files
/// send metadata only"). Name-based only - it does not inspect content, and the design note in that plan
/// section accepts this as a known, explicit-click-scoped limit.
pub fn is_secret_named(path: &str) -> bool {
    let lower = path.to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    base.starts_with(".env")
        || base.ends_with(".pem")
        || base.contains("key")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        is_git: bool,
        has_baseline: bool,
        baseline_mtime: Option<u64>,
        local_mtime: Option<u64>,
        remote_mtime: Option<u64>,
        local_size: Option<u64>,
        remote_size: Option<u64>,
    ) -> ClassifyInput {
        ClassifyInput { is_git, has_baseline, baseline_mtime, local_mtime, remote_mtime, local_size, remote_size }
    }

    // §2 table, row by row.

    #[test]
    fn git_path_is_always_git_group_regardless_of_every_other_input() {
        assert_eq!(
            classify_file(&input(true, false, None, None, None, None, None)),
            FileClass::GitGroup
        );
        assert_eq!(
            classify_file(&input(true, true, Some(100), Some(200), Some(200), Some(1), Some(2))),
            FileClass::GitGroup
        );
    }

    #[test]
    fn local_absent_in_baseline_is_push_local_deleted() {
        assert_eq!(
            classify_file(&input(false, true, Some(100), None, Some(999), None, None)),
            FileClass::Push
        );
    }

    #[test]
    fn local_absent_not_in_baseline_is_pull_remote_created() {
        assert_eq!(
            classify_file(&input(false, true, None, None, Some(999), None, None)),
            FileClass::Pull
        );
    }

    #[test]
    fn no_baseline_for_this_host_is_unclassified() {
        assert_eq!(
            classify_file(&input(false, false, None, Some(100), Some(200), Some(1), Some(1))),
            FileClass::Unclassified
        );
    }

    #[test]
    fn not_in_baseline_with_both_sides_present_is_unclassified() {
        assert_eq!(
            classify_file(&input(false, true, None, Some(100), Some(200), Some(1), Some(1))),
            FileClass::Unclassified
        );
    }

    #[test]
    fn l_eq_b_r_gt_b_is_pull() {
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(100), Some(500), None, None)),
            FileClass::Pull
        );
    }

    #[test]
    fn l_gt_b_r_eq_b_is_push() {
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(500), Some(100), None, None)),
            FileClass::Push
        );
    }

    #[test]
    fn r_lt_b_is_push_stale_remote_even_when_l_is_also_behind_baseline() {
        // "anything else" (L < B) would otherwise apply, but R < B (stale remote) wins per the table order, and is tagged PushStaleRemote (not plain Push) so the caller can surface "remote is behind".
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(50), Some(10), None, None)),
            FileClass::PushStaleRemote
        );
    }

    #[test]
    fn remote_absent_unedited_since_baseline_is_suppressed_remote_deleted() {
        // docs/feat/sync-flow.md §2's PUSH-suppression row, restored inside the pure classifier:
        // local mtime unchanged since the baseline and the remote has no metadata for this path at all
        // (never appeared in the pull-side diff) means the remote deleted it - not a push.
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(100), None, Some(1), None)),
            FileClass::Suppressed
        );
    }

    #[test]
    fn remote_absent_edited_since_baseline_is_a_real_push() {
        // Same "remote has no metadata" shape, but the local copy WAS edited since the baseline - a genuine local edit to push, not a suppressed remote deletion.
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(500), None, Some(1), None)),
            FileClass::Push
        );
    }

    #[test]
    fn l_gt_b_r_gt_b_sizes_differ_is_conflict() {
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(500), Some(600), Some(10), Some(20))),
            FileClass::Conflict
        );
    }

    #[test]
    fn l_gt_b_r_gt_b_same_size_needs_checksum() {
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(500), Some(600), Some(10), Some(10))),
            FileClass::NeedsChecksum
        );
    }

    #[test]
    fn l_gt_b_r_gt_b_missing_size_degrades_to_unclassified_never_guesses() {
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(500), Some(600), None, Some(10))),
            FileClass::Unclassified
        );
    }

    #[test]
    fn anything_else_l_lt_b_r_eq_b_is_unclassified() {
        assert_eq!(
            classify_file(&input(false, true, Some(100), Some(50), Some(100), None, None)),
            FileClass::Unclassified
        );
    }


    // --out-format parsing.

    // Every test below injects a literal offset closure (never `std::env::set_var`/the process's real TZ),
    // so running them in parallel with every other `cargo test` thread in this binary can never race - the
    // whole point of injecting the offset function rather than reading `local_utc_offset_secs_at` here.
    const UTC: &dyn Fn(i64) -> i64 = &|_| 0;

    #[test]
    fn parses_a_well_formed_out_format_line_at_utc() {
        let meta = parse_out_format_line("src/main.rs\t1234\t2024/01/02-15:04:05", UTC).unwrap();
        assert_eq!(meta.path, "src/main.rs");
        assert_eq!(meta.size, 1234);
        assert_eq!(meta.mtime, 1704207845);
    }

    #[test]
    fn known_epoch_anchors_round_trip_at_utc() {
        assert_eq!(parse_rsync_mtime("1970/01/01-00:00:00", UTC), Some(0));
        assert_eq!(parse_rsync_mtime("2000/01/01-00:00:00", UTC), Some(946684800));
        assert_eq!(parse_rsync_mtime("2038/01/19-03:14:07", UTC), Some(2147483647));
    }

    // %M is printed by rsync's own `timestring()`, which calls `localtime()` - the SAME wall-clock digits parse to a DIFFERENT UTC instant depending on the machine's zone.
    const ASIA_HO_CHI_MINH_OFFSET_SECS: i64 = 7 * 3600; // UTC+7, no DST
    const ASIA_HO_CHI_MINH: &dyn Fn(i64) -> i64 = &|_| ASIA_HO_CHI_MINH_OFFSET_SECS;

    #[test]
    fn same_wall_clock_digits_parse_to_different_utc_instants_under_different_offsets() {
        let digits = "2024/01/02-15:04:05";
        let at_utc = parse_rsync_mtime(digits, UTC).unwrap();
        let at_ho_chi_minh = parse_rsync_mtime(digits, ASIA_HO_CHI_MINH).unwrap();
        // UTC+7 wall-clock 15:04:05 is 7 hours EARLIER in real UTC than the same digits read as UTC itself.
        assert_eq!(at_utc - at_ho_chi_minh, ASIA_HO_CHI_MINH_OFFSET_SECS as u64);
    }

    #[test]
    fn a_local_edit_at_the_same_instant_as_the_baseline_does_not_misclassify_as_a_conflict_under_utc_plus_7() {
        // The exact bug shape #2 described: an unedited file (R == B in real UTC) must not read as R > B
        // just because the offset used to parse %M was wrong. Baseline B was recorded (by this app, in UTC
        // epoch seconds) at a real instant; rsync's %M for that SAME instant, printed in Asia/Ho_Chi_Minh
        // wall-clock time, must parse back to the same UTC second when given the right offset.
        let baseline_utc_epoch: u64 = 1_700_000_000; // an arbitrary fixed real UTC instant
        // The wall-clock digits a UTC+7 process would print for that exact instant.
        let local_wall_clock = "2023/11/15-05:13:20"; // 1_700_000_000 UTC + 7h, rendered as Y/M/D-H:M:S
        let parsed_back = parse_rsync_mtime(local_wall_clock, ASIA_HO_CHI_MINH).unwrap();
        assert_eq!(parsed_back, baseline_utc_epoch, "with the correct offset the round trip must be exact");
    }

    #[test]
    fn offset_is_looked_up_at_the_instant_being_decoded_not_at_a_fixed_moment_dst_safe() {
        // a single offset borrowed from "now" is wrong for a file whose real mtime falls on
        // the other side of a DST transition. This closure returns a DIFFERENT offset depending on which
        // rough half of the naive-local guess it is asked about - a synthetic Europe/Paris-like zone (UTC+1
        // winter, UTC+2 summer/DST) - proving `parse_rsync_mtime` re-queries the offset per value rather than
        // reusing one offset across every line it parses.
        const WINTER_OFFSET: i64 = 3600; // UTC+1
        const SUMMER_OFFSET: i64 = 7200; // UTC+2 (DST)
        // Strictly between the two fixtures below, so either branch alone would misclassify the wrong one.
        const MID_YEAR_GUESS: i64 = 1_688_000_000; // 2023-06-29, between the winter and summer fixtures
        let seasonal = |naive_local_guess: i64| -> i64 {
            if naive_local_guess < MID_YEAR_GUESS { WINTER_OFFSET } else { SUMMER_OFFSET }
        };

        // Winter: 2023-01-15 12:00:00 local, offset UTC+1 -> each instant must get its OWN season's offset,
        // not one borrowed from the other (the exact bug: a single process-wide offset applied to every
        // line regardless of that line's own date) - checked by comparing the seasonal closure's result
        // against parsing the same digits with that season's fixed offset alone.
        let winter_utc = parse_rsync_mtime("2023/01/15-12:00:00", &seasonal).unwrap();
        assert_eq!(winter_utc, parse_rsync_mtime("2023/01/15-12:00:00", &|_| WINTER_OFFSET).unwrap());

        // Summer: 2023/07/15 12:00:00 local, offset UTC+2 (DST).
        let summer_utc = parse_rsync_mtime("2023/07/15-12:00:00", &seasonal).unwrap();
        assert_eq!(summer_utc, parse_rsync_mtime("2023/07/15-12:00:00", &|_| SUMMER_OFFSET).unwrap());
    }

    #[test]
    fn rejects_a_directory_entry_line() {
        assert!(parse_out_format_line("src/\t0\t2024/01/02-15:04:05", UTC).is_none());
    }

    #[test]
    fn rejects_malformed_mtime() {
        assert!(parse_out_format_line("a.txt\t1\tnot-a-date", UTC).is_none());
    }

    #[test]
    fn rejects_wrong_column_count() {
        assert!(parse_out_format_line("a.txt\t1", UTC).is_none());
        assert!(parse_out_format_line("just-a-path", UTC).is_none());
    }

    #[test]
    fn parse_output_skips_status_lines_and_collects_files() {
        let stdout = "building file list ... done\n\
                       a.txt\t10\t2024/01/02-15:04:05\n\
                       sent 100 bytes  received 20 bytes\n\
                       sub/\t0\t2024/01/02-15:04:05\n\
                       b.txt\t20\t2024/06/01-00:00:00\n";
        let parsed = parse_out_format_output(stdout, UTC);
        assert!(!parsed.degraded);
        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.files[0].path, "a.txt");
        assert_eq!(parsed.files[1].path, "b.txt");
    }

    #[test]
    fn parse_output_degrades_on_a_plain_filename_line_no_out_format_support() {
        // e.g. an rsync build that silently ignored --out-format and printed its default itemized line.
        let stdout = "building file list ... done\na-plain-filename-with-no-tabs\n";
        let parsed = parse_out_format_output(stdout, UTC);
        assert!(parsed.degraded);
        assert!(parsed.files.is_empty());
    }

    // -u reapplication.

    #[test]
    fn select_pull_after_u_filter_matches_old_u_semantics_on_non_conflict_fixtures() {
        let metas = vec![
            RemoteFileMeta { path: "remote-newer.txt".into(), size: 1, mtime: 1000 },
            RemoteFileMeta { path: "local-newer.txt".into(), size: 1, mtime: 500 },
            RemoteFileMeta { path: "remote-created.txt".into(), size: 1, mtime: 2000 },
            // same mtime (within the modify-window), different size - real rsync `-u` only skips when the RECEIVER (local) is strictly newer, so this still counts as pull.
            RemoteFileMeta { path: "equal-mtime-diff-size.txt".into(), size: 999, mtime: 700 },
        ];
        let mut local = HashMap::new();
        local.insert("remote-newer.txt".to_string(), Some(400u64)); // remote (1000) newer -> pull
        local.insert("local-newer.txt".to_string(), Some(900u64)); // local (900) newer than remote (500) -> not pull
        local.insert("remote-created.txt".to_string(), None); // absent locally -> pull
        local.insert("equal-mtime-diff-size.txt".to_string(), Some(700u64)); // equal mtime, still differs -> pull
        let selected = select_pull_after_u_filter(&metas, &local);
        let paths: Vec<&str> = selected.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths.len(), 3, "old -u semantics: local-strictly-newer is the only exclusion");
        assert!(paths.contains(&"remote-newer.txt"));
        assert!(paths.contains(&"remote-created.txt"));
        assert!(paths.contains(&"equal-mtime-diff-size.txt"));
        assert!(!paths.contains(&"local-newer.txt"));
    }

    #[test]
    fn select_pull_after_u_filter_pull_count_matches_the_old_behaviour_on_non_conflict_fixtures() {
        // Same fixture, read as a plain count rather than by path.
        let metas = vec![
            RemoteFileMeta { path: "a".into(), size: 1, mtime: 1000 }, // remote newer -> pull
            RemoteFileMeta { path: "b".into(), size: 1, mtime: 500 },  // local newer -> not pull
            RemoteFileMeta { path: "c".into(), size: 5, mtime: 700 },  // equal mtime, diff size -> pull
        ];
        let mut local = HashMap::new();
        local.insert("a".to_string(), Some(400u64));
        local.insert("b".to_string(), Some(900u64));
        local.insert("c".to_string(), Some(700u64));
        let pull_count = select_pull_after_u_filter(&metas, &local).len();
        assert_eq!(pull_count, 2);
    }

    // Checksum residue cap.

    #[test]
    fn split_checksum_residue_under_cap_checks_everything() {
        let residue = vec!["a".to_string(), "b".to_string()];
        let (checked, unverified) = split_checksum_residue(residue, CHECKSUM_CAP);
        assert_eq!(checked.len(), 2);
        assert!(unverified.is_empty());
    }

    #[test]
    fn split_checksum_residue_exactly_at_cap_checks_everything() {
        let residue: Vec<String> = (0..CHECKSUM_CAP).map(|i| i.to_string()).collect();
        let (checked, unverified) = split_checksum_residue(residue, CHECKSUM_CAP);
        assert_eq!(checked.len(), CHECKSUM_CAP);
        assert!(unverified.is_empty());
    }

    #[test]
    fn split_checksum_residue_over_cap_marks_the_rest_unverified() {
        let residue: Vec<String> = (0..CHECKSUM_CAP + 5).map(|i| i.to_string()).collect();
        let (checked, unverified) = split_checksum_residue(residue, CHECKSUM_CAP);
        assert_eq!(checked.len(), CHECKSUM_CAP);
        assert_eq!(unverified.len(), 5);
    }

    // agy resolution and secret-name filtering.

    #[test]
    fn resolve_agy_bin_prefers_the_first_existing_candidate_in_order() {
        let existing = ["/home/u/.claude/local/agy", "/usr/local/bin/agy"];
        let found = resolve_agy_bin_with("/home/u", |c| existing.contains(&c));
        assert_eq!(found.as_deref(), Some("/home/u/.claude/local/agy"));
    }

    #[test]
    fn resolve_agy_bin_falls_through_every_candidate_to_none() {
        assert_eq!(resolve_agy_bin_with("/home/u", |_| false), None);
    }

    #[test]
    fn secret_named_files_are_detected_by_name() {
        assert!(is_secret_named(".env"));
        assert!(is_secret_named(".env.production"));
        assert!(is_secret_named("config/secrets.pem"));
        assert!(is_secret_named("id_rsa.key"));
        assert!(is_secret_named("src/keychain.js"));
    }

    #[test]
    fn ordinary_files_are_not_flagged_as_secret_named() {
        assert!(!is_secret_named("src/main.rs"));
        assert!(!is_secret_named("README.md"));
        assert!(!is_secret_named("package.json"));
    }

    #[test]
    fn classify_checksum_result_splits_converged_from_real_conflicts() {
        let checked = vec!["same.txt".to_string(), "diff.txt".to_string()];
        let mut differing = std::collections::HashSet::new();
        differing.insert("diff.txt".to_string());
        let (converged, conflicts) = classify_checksum_result(&checked, &differing);
        assert_eq!(converged, vec!["same.txt".to_string()]);
        assert_eq!(conflicts, vec!["diff.txt".to_string()]);
    }

    #[test]
    fn validate_conflict_rel_path_rejects_absolute() {
        assert!(validate_conflict_rel_path("/etc/passwd").is_err());
    }

    #[test]
    fn validate_conflict_rel_path_rejects_home_tilde() {
        assert!(validate_conflict_rel_path("~/secrets.txt").is_err());
    }

    #[test]
    fn validate_conflict_rel_path_rejects_traversal() {
        assert!(validate_conflict_rel_path("../../etc/passwd").is_err());
        assert!(validate_conflict_rel_path("a/../../b").is_err());
    }

    #[test]
    fn validate_conflict_rel_path_rejects_empty() {
        assert!(validate_conflict_rel_path("").is_err());
    }

    #[test]
    fn validate_conflict_rel_path_rejects_control_characters() {
        assert!(validate_conflict_rel_path("a\nb").is_err());
    }

    #[test]
    fn validate_conflict_rel_path_accepts_an_ordinary_relative_path() {
        assert!(validate_conflict_rel_path("src/components/App.vue").is_ok());
    }

    #[test]
    fn unified_diff_identical_text_is_all_context_lines() {
        let diff = unified_diff("a\nb\nc", "a\nb\nc");
        assert_eq!(diff, "  a\n  b\n  c");
    }

    #[test]
    fn unified_diff_marks_a_single_changed_line() {
        let diff = unified_diff("a\nb\nc", "a\nX\nc");
        assert_eq!(diff, "  a\n- b\n+ X\n  c");
    }

    #[test]
    fn unified_diff_marks_pure_additions_at_the_end() {
        let diff = unified_diff("a", "a\nb\nc");
        assert_eq!(diff, "  a\n+ b\n+ c");
    }

    #[test]
    fn unified_diff_marks_pure_deletions_at_the_end() {
        let diff = unified_diff("a\nb\nc", "a");
        assert_eq!(diff, "  a\n- b\n- c");
    }

    #[test]
    fn unified_diff_of_two_empty_texts_is_empty() {
        assert_eq!(unified_diff("", ""), "");
    }
}
