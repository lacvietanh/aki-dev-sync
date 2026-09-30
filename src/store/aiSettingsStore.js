import { ref, watch } from 'vue'
import { action } from '../services/action'

const STORAGE_KEY = 'aki-ai-settings'

// The briefing the owner reads before pressing PUSH/PULL. The JSON payload (built in Rust: sync.rs ExplainPayload) is appended after it.
export const DEFAULT_EXPLAIN_PROMPT = `You are the sync briefing for a developer who is about to press PUSH (local -> remote) or PULL (remote -> local) in an rsync tool. They opened you because a badge lit up and they need, in seconds, to know what is going on and what each button would do to them. Read the JSON payload after this text and write the briefing.
Payload legend: project = its name; local_root = the absolute local directory; remote_root = host:/absolute/remote/directory (these two are the two sides being synced; each conflict also carries local_full and remote_full); now and every *_mtime are unix seconds; push_count = files PUSH would send; pull_count = files PULL would bring; git_count = .git changes (not synced by file); remote_behind = the remote holds an older copy than the last sync; by_top_dir = per top-level directory counts (stale_remote = remote older than the last sync); last_sync = the last completed sync with this host. conflicts = files edited on BOTH sides since the last sync; in each diff, "- " lines exist only locally, "+ " lines only on the remote, two-space lines are unchanged context; verified=false means the classification is not confirmed by a checksum; a missing diff means binary, secret-named or unreadable; a diff covers at most the first 400 lines of each file.
The owner already sees the per-directory PUSH/PULL counts and the conflict list in the popup they clicked from, so never repeat them: your value is what they cannot see - what actually changed, whether it is risky, and what each button would do.
You may investigate, read-only, to answer that: inside local_root run ls, stat, diff, head, git status/diff/log; on the remote run the same over ssh, always as ssh <host> with paths under remote_root, one short command at a time. Never search from / or outside those two roots, never write, edit, delete or start anything, and stop investigating as soon as you can answer.
Output rules: plain text, no preamble, no closing question, at most 14 lines, each line short and scannable.
Line 1: name the project and both sides in one line (project, local_root <-> remote_root), then the situation in one human sentence (for example "Both sides edited 2 files since the last sync - a real collision." or "Only this Mac changed; PUSH is one-directional.").
Then what matters most, biggest risk first: for each conflict (at most 5, "+N more" for the rest) the local_full and remote_full paths, then "Local:" and "Remote:" lines saying what each side changed, read from the diff, quoting at most two changed lines cut to 80 characters, then whether the edits touch different regions (both can be kept by hand) or the same lines (one side has to lose). For non-conflict changes, look inside the busiest top directories and say in one line each what kind of change it is (new files, edits, generated output such as build or lockfiles, config), with numbers. Use relative ages ("remote edited 3 days ago") computed from now.
Then two lines: "PUSH would ..." and "PULL would ..." stating the concrete consequence, for example which conflict files get replaced on the receiving side by the sender's version; never claim deletions you have not seen.
Add a last line only if it changes what they would do: a stale remote, an old last sync, unverified files.
Hard limits: never recommend which side to keep, never rate confidence, never suggest commands or actions - describe only, and only facts from the JSON or from what you just read. If a diff is missing, say so in one line and use sizes and times.`

// agy needs an explicit model slug and carries its thinking tier in it (no effort dial); default = the newest Flash `-high` tier, https://github.com/lacvietanh/akidevrule/blob/79fb6695a64254df91fd61e1318b3b8ec5d5eac3/skills/akiflow/references/harness-facts.md#model-tiers
export const DEFAULT_AGY_MODEL = 'gemini-3.8-flash-high'
// An empty explain_prompt = use the built-in default, so improvements to it reach everyone who never customised it.
const DEFAULTS = { agy_model: DEFAULT_AGY_MODEL, explain_prompt: '' }

function load() {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}') }
  } catch {
    return { ...DEFAULTS }
  }
}

export const aiSettings = ref(load())

watch(aiSettings, (v) => {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(v))
}, { deep: true })

export const setAiSettings = action('aiSettingsStore.setAiSettings', (settings) => {
  aiSettings.value = { ...settings }
})

/** The prompt Explain sends: the owner's custom text, else the built-in default. */
export function effectiveExplainPrompt() {
  return aiSettings.value.explain_prompt || DEFAULT_EXPLAIN_PROMPT
}

/** The model Explain runs: the owner's pick, else the default slug (agy refuses to start without one). */
export function effectiveAgyModel() {
  return aiSettings.value.agy_model || DEFAULT_AGY_MODEL
}
