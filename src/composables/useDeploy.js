// Deploy as its own action (docs/plan/deploy-action.md): the ONE place that reads a project's deploy
// config, builds the local/remote command, shows the confirm dialog, and launches the in-app terminal tab
// - shared by the DEPLOY button (ProjectTable.vue) and the post-push `on_push` offer (useSync.js), so the
// two call sites can never drift on what "deploy" means (Law 1).
import { invoke } from '../utils/tauri'
import { askConfirm } from '../store/dialogStore'
import { Toast, projectRuntime } from '../store/projectStore'
import { useTerminalTabs } from './useTerminalTabs'
import { getDeployCmd, deployRunOn, deployTargetLabel, deployRemoteTarget, deployBlockedReason } from './projectConfigPure'
import { auditLog } from '../utils/auditLog'

// Pure predicates live in projectConfigPure.js (the one Tauri/Vue-free module `node --test` imports) - callers import them from there directly.

/**
 * The ONE impure resolver: `stack_info` never lives on the project object itself - it lives in
 * the separate `projectRuntime` store keyed by id (`useProjectStack.js`), so a caller-supplied detected
 * fallback is unavoidable. Every production caller (button, tooltip, D badge, post-push offer, confirm)
 * calls this, never the two-argument pure `getDeployCmd` directly - that direct call is test-only.
 */
export function resolveDeployCmd(project) {
  return getDeployCmd(project, projectRuntime.value[project?.id]?.stack_info?.deploy_cmd)
}

/**
 * Shows the one confirm dialog (exact command + target), then launches it: local runs in the project's own
 * in-app terminal tab (`openRunCommand`, same mechanism as DEV/BUILD); remote runs in a fresh SSH terminal
 * tab via `build_remote_deploy_command` on the deploy's OWN host/path (`deployRemoteTarget`), never the
 * active sync host. Deploy has
 * no ignore-errors option (deploy plan) - a failure is simply visible in the tab, same as DEV/BUILD.
 */
export async function confirmAndDeploy(project, cmd) {
  const blocked = deployBlockedReason(project)
  if (blocked) {
    Toast.fire({ icon: 'error', title: blocked })
    return
  }
  const target = deployTargetLabel(project)
  const answer = await askConfirm({
    kind: 'confirm',
    title: 'Deploy?',
    text: `Run "${cmd}" on ${target}?`,
    icon: 'question',
    confirmButtonText: 'Deploy',
    cancelButtonText: 'Cancel',
  })
  if (!answer?.confirmed) return
  auditLog('deploy', `${project.id} (${project.local_path}) run "${cmd}" on ${target}`)
  const { openRunCommand, openProjectRemoteTerminal } = useTerminalTabs()
  if (deployRunOn(project) === 'remote') {
    try {
      const { host, path } = deployRemoteTarget(project)
      const sshCmd = await invoke('build_remote_deploy_command', { host, path, cmd })
      openProjectRemoteTerminal(project, sshCmd)
    } catch (e) {
      console.error('[useDeploy] build_remote_deploy_command failed', e)
      Toast.fire({ icon: 'error', title: String(e).replace('Error: ', '') })
    }
  } else {
    openRunCommand(project, cmd, 'deploy')
  }
}

/** Post-push offer (deploy plan § "Always ask first"): the caller (`useSync.js`) already decided to offer
 * via `shouldOfferDeployAfterPush` - this just runs the one confirm-and-launch flow with the resolved cmd,
 * never re-checking `deployOnPush`/the command again (that duplication is exactly the
 * `pattern.A8` gap the offer decision used to drift on). */
export async function offerDeployAfterPush(project, cmd) {
  await confirmAndDeploy(project, cmd)
}
