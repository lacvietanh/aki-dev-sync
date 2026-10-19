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

/// Per-poll limit on uncached same-size files sent to `rsync -c` (docs/feat/sync-flow.md § Checksum residue).
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
    /// A baseline exists AND was recorded against the target's current `remote_path`; the caller derives it from `baseline_for_target`, never this module.
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
    /// A push because the remote regressed (R < B); counted like `Push`, kept separate to surface "remote is behind".
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
    /// Remote deleted it and the local copy is unedited since the baseline (sync-flow.md §2's suppression row): in no count, never a conflict.
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
        // Absent on the remote: unedited locally since the baseline means a remote deletion (suppress), edited means a real push.
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

/// Parses one `path\tsize\tYYYY/MM/DD-HH:MM:SS` line; `None` means this rsync's `--out-format` is not what we expect (degrade), never a zero-size epoch file.
pub fn parse_out_format_line(
    line: &str,
    offset_secs_at: &dyn Fn(i64) -> i64,
) -> Option<RemoteFileMeta> {
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
    Some(RemoteFileMeta {
        path: path.to_string(),
        size,
        mtime,
    })
}

/// `%M` (the local rsync client's wall-clock time) to UTC, with the offset valid at that instant, not now (DST-safe); the offset is injected to keep this pure.
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
    /// A file-like line failed to parse as `path\tsize\tmtime`: the unknown-format degrade path (docs/plan/done/conflict-detection-and-agy-report.md § Execution steps).
    pub degraded: bool,
}

pub fn parse_out_format_output(
    stdout: &str,
    offset_secs_at: &dyn Fn(i64) -> i64,
) -> ParsedRemoteDiff {
    let mut files = Vec::new();
    let mut degraded = false;
    for line in stdout.lines() {
        let l = line.trim();
        if l.is_empty() || RSYNC_STATUS_PREFIXES.iter().any(|p| l.starts_with(p)) {
            continue;
        }
        // Split first so a directory entry (path ends in `/`) is skipped as valid; `parse_out_format_line` would read it as a degrade.
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

/// This machine's UTC offset (seconds east) valid at `unix_ts_guess`, via `tm_gmtoff`; the one real caller of the injected offset. Never call it from a test.
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

/// Reapplies the pull side's `-u` in Rust, since the dry-run drops it to get every file's metadata (docs/plan/done/conflict-detection-and-agy-report.md). `None` in `local_mtimes` = absent locally.
pub fn select_pull_after_u_filter<'a>(
    metas: &'a [RemoteFileMeta],
    local_mtimes: &HashMap<String, Option<u64>>,
) -> Vec<&'a RemoteFileMeta> {
    metas
        .iter()
        .filter(|m| match local_mtimes.get(&m.path) {
            // Real `-u` skips only when the receiver is strictly newer (beyond the modify-window); an equal-mtime, different-size pair is still pulled.
            Some(Some(local)) => !mtime_gt(*local, m.mtime),
            Some(None) | None => true, // absent locally - remote-created, always a pull candidate
        })
        .collect()
}

/// The first `cap` uncached same-size files get a checksum call; the rest stay unchecked, never trusted.
pub fn split_checksum_residue(residue: Vec<String>, cap: usize) -> (Vec<String>, Vec<String>) {
    if residue.len() <= cap {
        (residue, Vec::new())
    } else {
        let mut checked = residue;
        let unverified = checked.split_off(cap);
        (checked, unverified)
    }
}

/// After the `rsync -c` call: a listed file differs in content (a real conflict); an unlisted one converged and leaves every count.
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

/// Finds `agy` at static install dirs first (cold-start PATH race, tauri.A2), pure over `exists` so the order is testable; the caller adds `command -v`.
pub fn resolve_agy_bin_with(home: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let candidates = [
        format!("{}/.local/bin/agy", home),
        format!("{}/.claude/local/agy", home),
        "/opt/homebrew/bin/agy".to_string(),
        "/usr/local/bin/agy".to_string(),
    ];
    candidates.into_iter().find(|c| exists(c))
}

/// Checks an IPC-supplied conflict path (relative, no `..`, no control chars) before it joins a temp dir or a file read: IPC is untrusted whatever the origin.
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
        return Err(format!(
            "conflict path '{}' contains control characters",
            rel
        ));
    }
    Ok(())
}

/// Compact LCS diff of two excerpts for `agy` to narrate (no hunk headers, not a patch); no diff crate for one call site.
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

/// A name whose content never goes to `agy` on Explain (metadata only); name-based by design (docs/plan/done/conflict-detection-and-agy-report.md § agy explanation).
pub fn is_secret_named(path: &str) -> bool {
    let lower = path.to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    base.starts_with(".env") || base.ends_with(".pem") || base.contains("key")
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
        ClassifyInput {
            is_git,
            has_baseline,
            baseline_mtime,
            local_mtime,
            remote_mtime,
            local_size,
            remote_size,
        }
    }

    // §2 table, row by row.

    #[test]
    fn git_path_is_always_git_group_regardless_of_every_other_input() {
        assert_eq!(
            classify_file(&input(true, false, None, None, None, None, None)),
            FileClass::GitGroup
        );
        assert_eq!(
            classify_file(&input(
                true,
                true,
                Some(100),
                Some(200),
                Some(200),
                Some(1),
                Some(2)
            )),
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
            classify_file(&input(
                false,
                false,
                None,
                Some(100),
                Some(200),
                Some(1),
                Some(1)
            )),
            FileClass::Unclassified
        );
    }

    #[test]
    fn not_in_baseline_with_both_sides_present_is_unclassified() {
        assert_eq!(
            classify_file(&input(
                false,
                true,
                None,
                Some(100),
                Some(200),
                Some(1),
                Some(1)
            )),
            FileClass::Unclassified
        );
    }

    #[test]
    fn l_eq_b_r_gt_b_is_pull() {
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(100),
                Some(500),
                None,
                None
            )),
            FileClass::Pull
        );
    }

    #[test]
    fn l_gt_b_r_eq_b_is_push() {
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(500),
                Some(100),
                None,
                None
            )),
            FileClass::Push
        );
    }

    #[test]
    fn r_lt_b_is_push_stale_remote_even_when_l_is_also_behind_baseline() {
        // R < B wins over the L < B row per table order, tagged `PushStaleRemote` so the caller can say "remote is behind".
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(50),
                Some(10),
                None,
                None
            )),
            FileClass::PushStaleRemote
        );
    }

    #[test]
    fn remote_absent_unedited_since_baseline_is_suppressed_remote_deleted() {
        // sync-flow.md §2's suppression row: unchanged locally and absent from the pull diff means a remote deletion, not a push.
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(100),
                None,
                Some(1),
                None
            )),
            FileClass::Suppressed
        );
    }

    #[test]
    fn remote_absent_edited_since_baseline_is_a_real_push() {
        // Same "remote has no metadata" shape, but the local copy WAS edited since the baseline - a genuine local edit to push, not a suppressed remote deletion.
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(500),
                None,
                Some(1),
                None
            )),
            FileClass::Push
        );
    }

    #[test]
    fn l_gt_b_r_gt_b_sizes_differ_is_conflict() {
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(500),
                Some(600),
                Some(10),
                Some(20)
            )),
            FileClass::Conflict
        );
    }

    #[test]
    fn l_gt_b_r_gt_b_same_size_needs_checksum() {
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(500),
                Some(600),
                Some(10),
                Some(10)
            )),
            FileClass::NeedsChecksum
        );
    }

    #[test]
    fn l_gt_b_r_gt_b_missing_size_degrades_to_unclassified_never_guesses() {
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(500),
                Some(600),
                None,
                Some(10)
            )),
            FileClass::Unclassified
        );
    }

    #[test]
    fn anything_else_l_lt_b_r_eq_b_is_unclassified() {
        assert_eq!(
            classify_file(&input(
                false,
                true,
                Some(100),
                Some(50),
                Some(100),
                None,
                None
            )),
            FileClass::Unclassified
        );
    }

    // --out-format parsing.

    // Literal offset closures only, never the process TZ, so these tests cannot race other threads.
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
        assert_eq!(
            parse_rsync_mtime("2000/01/01-00:00:00", UTC),
            Some(946684800)
        );
        assert_eq!(
            parse_rsync_mtime("2038/01/19-03:14:07", UTC),
            Some(2147483647)
        );
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
    fn a_local_edit_at_the_same_instant_as_the_baseline_does_not_misclassify_as_a_conflict_under_utc_plus_7(
    ) {
        // An unedited file (R == B in UTC) must not read as R > B: `%M` printed in Asia/Ho_Chi_Minh time parses back to the same UTC second.
        let baseline_utc_epoch: u64 = 1_700_000_000; // an arbitrary fixed real UTC instant
                                                     // The wall-clock digits a UTC+7 process would print for that exact instant.
        let local_wall_clock = "2023/11/15-05:13:20"; // 1_700_000_000 UTC + 7h, rendered as Y/M/D-H:M:S
        let parsed_back = parse_rsync_mtime(local_wall_clock, ASIA_HO_CHI_MINH).unwrap();
        assert_eq!(
            parsed_back, baseline_utc_epoch,
            "with the correct offset the round trip must be exact"
        );
    }

    #[test]
    fn offset_is_looked_up_at_the_instant_being_decoded_not_at_a_fixed_moment_dst_safe() {
        // A synthetic Paris-like zone (UTC+1 winter, UTC+2 summer) proves the offset is queried per value, not reused across lines.
        const WINTER_OFFSET: i64 = 3600; // UTC+1
        const SUMMER_OFFSET: i64 = 7200; // UTC+2 (DST)
                                         // Strictly between the two fixtures below, so either branch alone would misclassify the wrong one.
        const MID_YEAR_GUESS: i64 = 1_688_000_000; // 2023-06-29, between the winter and summer fixtures
        let seasonal = |naive_local_guess: i64| -> i64 {
            if naive_local_guess < MID_YEAR_GUESS {
                WINTER_OFFSET
            } else {
                SUMMER_OFFSET
            }
        };

        // Each instant must get its own season's offset, checked against parsing the same digits with that season's fixed offset.
        let winter_utc = parse_rsync_mtime("2023/01/15-12:00:00", &seasonal).unwrap();
        assert_eq!(
            winter_utc,
            parse_rsync_mtime("2023/01/15-12:00:00", &|_| WINTER_OFFSET).unwrap()
        );

        // Summer: 2023/07/15 12:00:00 local, offset UTC+2 (DST).
        let summer_utc = parse_rsync_mtime("2023/07/15-12:00:00", &seasonal).unwrap();
        assert_eq!(
            summer_utc,
            parse_rsync_mtime("2023/07/15-12:00:00", &|_| SUMMER_OFFSET).unwrap()
        );
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
            RemoteFileMeta {
                path: "remote-newer.txt".into(),
                size: 1,
                mtime: 1000,
            },
            RemoteFileMeta {
                path: "local-newer.txt".into(),
                size: 1,
                mtime: 500,
            },
            RemoteFileMeta {
                path: "remote-created.txt".into(),
                size: 1,
                mtime: 2000,
            },
            // same mtime (within the modify-window), different size - real rsync `-u` only skips when the RECEIVER (local) is strictly newer, so this still counts as pull.
            RemoteFileMeta {
                path: "equal-mtime-diff-size.txt".into(),
                size: 999,
                mtime: 700,
            },
        ];
        let mut local = HashMap::new();
        local.insert("remote-newer.txt".to_string(), Some(400u64)); // remote (1000) newer -> pull
        local.insert("local-newer.txt".to_string(), Some(900u64)); // local (900) newer than remote (500) -> not pull
        local.insert("remote-created.txt".to_string(), None); // absent locally -> pull
        local.insert("equal-mtime-diff-size.txt".to_string(), Some(700u64)); // equal mtime, still differs -> pull
        let selected = select_pull_after_u_filter(&metas, &local);
        let paths: Vec<&str> = selected.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(
            paths.len(),
            3,
            "old -u semantics: local-strictly-newer is the only exclusion"
        );
        assert!(paths.contains(&"remote-newer.txt"));
        assert!(paths.contains(&"remote-created.txt"));
        assert!(paths.contains(&"equal-mtime-diff-size.txt"));
        assert!(!paths.contains(&"local-newer.txt"));
    }

    #[test]
    fn select_pull_after_u_filter_pull_count_matches_the_old_behaviour_on_non_conflict_fixtures() {
        // Same fixture, read as a plain count rather than by path.
        let metas = vec![
            RemoteFileMeta {
                path: "a".into(),
                size: 1,
                mtime: 1000,
            }, // remote newer -> pull
            RemoteFileMeta {
                path: "b".into(),
                size: 1,
                mtime: 500,
            }, // local newer -> not pull
            RemoteFileMeta {
                path: "c".into(),
                size: 5,
                mtime: 700,
            }, // equal mtime, diff size -> pull
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
