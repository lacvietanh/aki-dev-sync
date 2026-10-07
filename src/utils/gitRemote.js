// Browser URL for a git remote: `git@host:path`, `ssh://[user@]host[:port]/path` and HTTPS remotes all map to `https://host/path`, credentials and `.git` dropped. '' when the remote is not a host URL (a local path).
export function remoteToWebUrl(remote) {
  const trimmed = (remote || '').trim()
  const scp = trimmed.match(/^[^@/\s]+@([^:/\s]+):(?!\/)(.+)$/)
  const url = scp ? null : trimmed.match(/^(?:ssh|git|https?):\/\/(?:[^@/]+@)?([^/:]+)(?::\d+)?\/(.+)$/)
  const match = scp || url
  if (!match) return ''
  const [, host, path] = match
  return `https://${host}/${path.replace(/\.git\/?$/, '').replace(/\/$/, '')}`
}
