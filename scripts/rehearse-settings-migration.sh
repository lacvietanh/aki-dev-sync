#!/usr/bin/env bash
# Rehearses the 1.32.0 settings/state migration (docs/plan/settings-and-state-layout.md,
# sync_state.rs::migrate_settings_and_state) against a COPY of a real ~/.aki/devsync tree, never the
# original (release.B5: "rehearse from the PREVIOUS state, never from empty"). The actual migration run
# and its postcondition asserts live in the Rust test this script drives -
# `sync_state.rs::tests::rehearse_migration_against_a_real_devsync_copy` (#[ignore], gated on the
# AKI_REHEARSAL_DEVSYNC_COPY env var this script sets) - so the counts printed here and the ones the test
# asserts against are the same code path, not two hand-kept numbers.
#
# Usage:
#   ./scripts/rehearse-settings-migration.sh <source-devsync-dir> [--legacy-baselines <dir>]
#   ./scripts/rehearse-settings-migration.sh ~/.aki/devsync.backup-pre-1.32.0 \
#     --legacy-baselines ~/.aki/devsync-baselines.backup-pre-1.32.0            # the real backups, § 1
#   ./scripts/rehearse-settings-migration.sh <dir> --allow-migrated             # source already has state/
#   ./scripts/rehearse-settings-migration.sh <dir> --keep                      # keep the temp copy always
#
# <source-devsync-dir> is REQUIRED and there is no default - pass a backup or a fixture, never the app's
# own real data directory (docs/plan/1.32.0-mac-handoff.md § 1's backup command is what produces the path
# you pass here). --legacy-baselines is likewise REQUIRED if the migration needs to read that directory -
# there is no auto-detected default pointing at the live ~/.aki/devsync-baselines, for the same reason
# there is none for the source. Both are only ever READ from, once, via `cp -R`.

set -euo pipefail

SOURCE=""
KEEP=0
ALLOW_MIGRATED=0
LEGACY_BASELINES=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep) KEEP=1; shift ;;
    --allow-migrated) ALLOW_MIGRATED=1; shift ;;
    --legacy-baselines) LEGACY_BASELINES="$2"; shift 2 ;;
    -h|--help) grep '^#' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown flag: $1" >&2; exit 1 ;;
    *) SOURCE="$1"; shift ;;
  esac
done

if [[ -z "$SOURCE" ]]; then
  echo "ERROR: pass the source devsync directory to rehearse from (e.g. a ~/.aki/devsync.backup-* copy)." >&2
  echo "Usage: $0 <source-devsync-dir> [--allow-migrated] [--keep] [--legacy-baselines <dir>]" >&2
  exit 1
fi
if [[ ! -d "$SOURCE" ]]; then
  echo "ERROR: source directory not found: $SOURCE" >&2
  exit 1
fi
if [[ ! -f "$SOURCE/projects.json" ]]; then
  echo "ERROR: $SOURCE/projects.json not found - this does not look like an Aki Dev Sync data directory" >&2
  exit 1
fi
if [[ -d "$SOURCE/state" && "$ALLOW_MIGRATED" -ne 1 ]]; then
  echo "ERROR: $SOURCE/state already exists - this source has already been migrated, so rehearsing against" >&2
  echo "it would prove nothing about the previous state (release.B5: rehearse from the PREVIOUS state)." >&2
  echo "Pass --allow-migrated to run anyway (e.g. to re-check idempotency on an already-migrated copy)." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TMP_ROOT="$(mktemp -d)"
COPY="$TMP_ROOT/devsync"
echo "Copying $SOURCE -> $COPY (read-only against the source; nothing under $SOURCE is ever modified)"
cp -R "$SOURCE" "$COPY"

LEGACY_COPY=""
if [[ -n "$LEGACY_BASELINES" ]]; then
  LEGACY_COPY="$TMP_ROOT/devsync-baselines"
  echo "Copying $LEGACY_BASELINES -> $LEGACY_COPY (pre-1.7.1 fallback baselines the migration also reads)"
  cp -R "$LEGACY_BASELINES" "$LEGACY_COPY"
fi

cleanup() {
  if [[ "$KEEP" -eq 1 ]]; then
    echo "Copy kept at $TMP_ROOT for inspection (--keep)."
  else
    rm -rf "$TMP_ROOT"
  fi
}
trap cleanup EXIT

echo "Running the migration rehearsal test against the copy..."
set +e
(
  cd "$REPO_ROOT/src-tauri"
  export AKI_REHEARSAL_DEVSYNC_COPY="$COPY"
  if [[ -n "$LEGACY_COPY" ]]; then
    export AKI_REHEARSAL_LEGACY_BASELINES_COPY="$LEGACY_COPY"
  fi
  cargo test --lib -- --ignored --nocapture rehearse_migration_against_a_real_devsync_copy
)
STATUS=$?
set -e

if [[ $STATUS -eq 0 ]]; then
  echo "REHEARSAL PASSED."
else
  echo "REHEARSAL FAILED (exit $STATUS) - see the test output above for which assertion failed." >&2
  KEEP=1
fi
exit $STATUS
