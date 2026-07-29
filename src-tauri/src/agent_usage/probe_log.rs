use crate::logger;

/// Truncate a string for safe log preview (no newlines, bounded length).
pub(crate) fn preview(s: &str, max: usize) -> String {
    let s = s.trim();
    let s = if s.len() > max {
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
pub(crate) fn ab(agent: &str) -> &str {
    match agent {
        "claudecode" => "CC",
        "antigravity" => "AG",
        other => other,
    }
}

/// Emit each non-empty shell stderr line at debug level.
pub(crate) fn log_shell_stderr(tag: &str, stderr: &str) {
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
