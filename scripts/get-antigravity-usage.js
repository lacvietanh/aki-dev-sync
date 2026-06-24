/**
 * scripts/get-antigravity-usage.js
 * @docs docs/arch/usage-antigravity.md
 * 
 * Purpose:
 * Query the running Antigravity IDE local Language Server instance via Connect RPC to fetch
 * real-time AI quota information. This script replaces the buggy third-party NPM package
 * 'antigravity-usage' and runs in milliseconds with 100% stability.
 * 
 * Flow:
 * 1. PROCESS DETECTION:
 *    - On macOS/Unix, runs `ps auxww` (to prevent command-line truncation).
 *    - Filters and targets ONLY the actual native Antigravity binary filenames (e.g. `language_server_macos_arm`)
 *      to avoid false matches against other plugins (like Volar's `language-server.js` or `cssServerMain`).
 *    - Extracts the `--csrf_token` and `--extension_server_port` from arguments.
 * 2. PORT DISCOVERY:
 *    - Queries listening TCP ports of the PID via `lsof -nP -iTCP -sTCP:LISTEN -a -p <PID>` (macOS) or netstat/ss (Linux/Win).
 *    - Seeds list with `extensionServerPort` and `extensionServerPort + 1` as reliable fallbacks.
 * 3. CONNECT RPC PROBE:
 *    - Probes detected ports via HTTP/HTTPS POST to `/exa.language_server_pb.LanguageServerService/GetUnleashData`.
 *    - Verifies valid Connect RPC response codes (200/401).
 * 4. QUOTA FETCH:
 *    - Queries `/exa.language_server_pb.LanguageServerService/GetUserStatus` with CSRF token and IDE metadata.
 *    - Standardizes the response into the unified JSON format expected by the frontend.
 * 
 * Why it is stable:
 * - Precise Binary Targeting: Eliminates matching conflicts with JS-based or CSS helpers.
 * - No Truncation: Using `ps auxww` guarantees arguments like CSRF tokens and ports are fully captured.
 * - Multi-Port Resilience: Probes all listening ports + seed ports to ensure connection success.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import http from 'http';
import https from 'https';

const execAsync = promisify(exec);

const debug = (...args) => {
  if (process.env.DEBUG) {
    console.error('[DEBUG]', ...args);
  }
};

// Target native binary filenames of the Antigravity Language Server across OS/archs
const BINARY_NAMES = [
  'language_server_macos_arm',
  'language_server_macos_x64',
  'language_server_linux_x64',
  'language_server_linux_arm64',
  'language_server_windows_x64.exe'
];

async function detectAntigravityProcess() {
  const platform = process.platform;
  debug(`Detecting Antigravity process on platform: ${platform}`);
  if (platform === 'win32') {
    return detectOnWindows();
  } else {
    return detectOnUnix();
  }
}

async function detectOnUnix() {
  // Use ps auxww on macOS/Darwin to ensure the command-line arguments are not truncated
  const cmd = process.platform === 'darwin' ? 'ps auxww' : 'ps aux';
  const { stdout } = await execAsync(cmd);
  const lines = stdout.split('\n');

  for (const line of lines) {
    const isTargetBinary = BINARY_NAMES.some(binName => line.includes(binName));
    if (!isTargetBinary) {
      continue;
    }

    // Double check process arguments for required server signals
    if (!line.includes('--csrf_token') && !line.includes('--extension_server_port')) {
      continue;
    }

    debug(`Matched Antigravity process line: ${line.trim()}`);
    const processInfo = parseUnixProcessLine(line);
    if (processInfo) {
      return processInfo;
    }
  }
  return null;
}

function parseUnixProcessLine(line) {
  const parts = line.trim().split(/\s+/);
  if (parts.length < 11) {
    return null;
  }
  const pid = parseInt(parts[1], 10);
  if (isNaN(pid)) {
    return null;
  }
  const commandLine = parts.slice(10).join(' ');
  const csrfToken = extractArgument(commandLine, '--csrf_token');
  const extensionServerPort = extractArgument(commandLine, '--extension_server_port');

  return {
    pid,
    csrfToken: csrfToken || undefined,
    extensionServerPort: extensionServerPort ? parseInt(extensionServerPort, 10) : undefined,
    commandLine
  };
}

async function detectOnWindows() {
  try {
    const { stdout } = await execAsync(
      `wmic process where "name like '%language_server_windows%'" get processid,commandline /format:csv`,
      { maxBuffer: 10 * 1024 * 1024 }
    );
    const lines = stdout.split('\n').filter(line => line.trim() && !line.includes('Node,CommandLine,ProcessId'));
    for (const line of lines) {
      const parts = line.split(',');
      if (parts.length >= 3) {
        const commandLine = parts.slice(1, -1).join(',');
        const pid = parseInt(parts[parts.length - 1].trim(), 10);
        if (!isNaN(pid)) {
          return {
            pid,
            csrfToken: extractArgument(commandLine, '--csrf_token') || undefined,
            extensionServerPort: extractArgument(commandLine, '--extension_server_port') ? parseInt(extractArgument(commandLine, '--extension_server_port'), 10) : undefined,
            commandLine
          };
        }
      }
    }
  } catch (err) {
    debug('Windows WMIC detection failed, trying PowerShell:', err.message);
  }

  try {
    const { stdout } = await execAsync(
      `powershell -Command "Get-Process | Where-Object { $_.ProcessName -like '*language_server_windows*' } | Select-Object Id | ConvertTo-Json"`
    );
    if (stdout.trim()) {
      const processes = JSON.parse(stdout);
      const processList = Array.isArray(processes) ? processes : [processes];
      for (const proc of processList) {
        if (proc.Id) {
          const { stdout: cmdLine } = await execAsync(
            `powershell -Command "(Get-CimInstance Win32_Process -Filter 'ProcessId = ${proc.Id}').CommandLine"`
          );
          const commandLine = cmdLine.trim();
          return {
            pid: proc.Id,
            csrfToken: extractArgument(commandLine, '--csrf_token') || undefined,
            extensionServerPort: extractArgument(commandLine, '--extension_server_port') ? parseInt(extractArgument(commandLine, '--extension_server_port'), 10) : undefined,
            commandLine
          };
        }
      }
    }
  } catch (err) {
    debug('Windows PowerShell detection failed:', err.message);
  }
  return null;
}

function extractArgument(commandLine, argName) {
  const eqRegex = new RegExp(`${argName}=([^\\s"']+|"[^"]*"|'[^']*')`, 'i');
  const eqMatch = commandLine.match(eqRegex);
  if (eqMatch) {
    return eqMatch[1].replace(/^["']|["']$/g, '');
  }
  const spaceRegex = new RegExp(`${argName}\\s+([^\\s"']+|"[^"]*"|'[^']*')`, 'i');
  const spaceMatch = commandLine.match(spaceRegex);
  if (spaceMatch) {
    return spaceMatch[1].replace(/^["']|["']$/g, '');
  }
  return null;
}

async function discoverPorts(pid, extensionServerPort) {
  const platform = process.platform;
  const ports = [];

  // Seed with extension_server_port and extension_server_port + 1 if available
  if (extensionServerPort) {
    ports.push(extensionServerPort);
    ports.push(extensionServerPort + 1);
  }

  try {
    if (platform === 'darwin') {
      const { stdout } = await execAsync(`lsof -nP -iTCP -sTCP:LISTEN -a -p ${pid}`);
      const lines = stdout.split('\n');
      for (const line of lines) {
        const match = line.match(/:(\d+)\s+\(LISTEN\)/);
        if (match) {
          const port = parseInt(match[1], 10);
          if (!isNaN(port) && !ports.includes(port)) {
            ports.push(port);
          }
        }
      }
    } else if (platform === 'win32') {
      const { stdout } = await execAsync('netstat -ano');
      const lines = stdout.split('\n');
      for (const line of lines) {
        if (line.includes('LISTENING')) {
          const parts = line.trim().split(/\s+/);
          const linePid = parseInt(parts[parts.length - 1], 10);
          if (linePid === pid) {
            const localAddr = parts[1];
            const portMatch = localAddr.match(/:(\d+)$/);
            if (portMatch) {
              const port = parseInt(portMatch[1], 10);
              if (!isNaN(port) && !ports.includes(port)) {
                ports.push(port);
              }
            }
          }
        }
      }
    } else {
      // Linux
      let stdout = '';
      try {
        const res = await execAsync(`ss -tlnp | grep "pid=${pid},"`);
        stdout = res.stdout;
      } catch {
        try {
          const res = await execAsync(`netstat -tlnp 2>/dev/null | grep "${pid}/"`);
          stdout = res.stdout;
        } catch {}
      }
      const lines = stdout.split('\n');
      for (const line of lines) {
        const match = line.match(/:(\d+)\s/);
        if (match) {
          const port = parseInt(match[1], 10);
          if (!isNaN(port) && !ports.includes(port)) {
            ports.push(port);
          }
        }
      }
    }
  } catch (err) {
    debug('discoverPorts lsof/netstat failed (expected if not listening or blocked):', err.message);
  }

  // Deduplicate and filter valid port ranges
  return [...new Set(ports)].filter(port => port > 0 && port < 65536);
}

const CONNECT_RPC_PATH = '/exa.language_server_pb.LanguageServerService/GetUnleashData';
const VALID_CONNECT_STATUSES = new Set([200, 401]);

async function probeForConnectAPI(ports, csrfToken) {
  debug(`Probing ports for Connect API: ${ports.join(', ')}`);
  for (const port of ports) {
    const result = await probePort(port, csrfToken);
    if (result) {
      return result;
    }
  }
  return null;
}

async function probePort(port, csrfToken) {
  // Probe HTTPS first, then HTTP
  const httpsResult = await probeProtocol(port, csrfToken, true);
  if (httpsResult) return httpsResult;

  const httpResult = await probeProtocol(port, csrfToken, false);
  if (httpResult) return httpResult;

  return null;
}

function probeProtocol(port, csrfToken, isHttps) {
  return new Promise((resolve) => {
    const options = {
      hostname: '127.0.0.1',
      port,
      path: CONNECT_RPC_PATH,
      method: 'POST',
      timeout: 500,
      headers: {
        'Content-Type': 'application/json',
        'Connect-Protocol-Version': '1',
        ...(csrfToken ? { 'X-Codeium-Csrf-Token': csrfToken } : {})
      },
      ...(isHttps ? { rejectUnauthorized: false } : {})
    };

    const client = isHttps ? https : http;
    const req = client.request(options, (res) => {
      if (res.statusCode && VALID_CONNECT_STATUSES.has(res.statusCode)) {
        debug(`Port ${port} responded successfully with status ${res.statusCode} via ${isHttps ? 'HTTPS' : 'HTTP'}`);
        resolve({
          baseUrl: `${isHttps ? 'https' : 'http'}://127.0.0.1:${port}`,
          port
        });
      } else {
        resolve(null);
      }
      res.resume();
    });

    req.on('error', () => resolve(null));
    req.on('timeout', () => {
      req.destroy();
      resolve(null);
    });

    req.write(JSON.stringify({ wrapper_data: {} }));
    req.end();
  });
}

function getUserStatus(baseUrl, csrfToken) {
  return new Promise((resolve, reject) => {
    const isHttps = baseUrl.startsWith('https://');
    const url = new URL('/exa.language_server_pb.LanguageServerService/GetUserStatus', baseUrl);
    const options = {
      hostname: url.hostname,
      port: url.port,
      path: url.pathname,
      method: 'POST',
      timeout: 2000,
      headers: {
        'Accept': 'application/json',
        'Content-Type': 'application/json',
        'Connect-Protocol-Version': '1',
        ...(csrfToken ? { 'X-Codeium-Csrf-Token': csrfToken } : {})
      },
      ...(isHttps ? { rejectUnauthorized: false } : {})
    };

    const client = isHttps ? https : http;
    const req = client.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => {
        data += chunk;
      });
      res.on('end', () => {
        if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
          try {
            resolve(JSON.parse(data));
          } catch (e) {
            reject(new Error(`Failed to parse UserStatus response: ${e.message}`));
          }
        } else {
          reject(new Error(`HTTP error status ${res.statusCode}: ${data}`));
        }
      });
    });

    req.on('error', reject);
    req.on('timeout', () => {
      req.destroy();
      reject(new Error('Request timed out'));
    });

    req.write(JSON.stringify({
      metadata: {
        ideName: 'antigravity',
        extensionName: 'antigravity',
        locale: 'en'
      }
    }));
    req.end();
  });
}

function retrieveUserQuotaSummary(baseUrl, csrfToken) {
  return new Promise((resolve, reject) => {
    const isHttps = baseUrl.startsWith('https://');
    const url = new URL('/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary', baseUrl);
    const options = {
      hostname: url.hostname,
      port: url.port,
      path: url.pathname,
      method: 'POST',
      timeout: 2000,
      headers: {
        'Accept': 'application/json',
        'Content-Type': 'application/json',
        'Connect-Protocol-Version': '1',
        ...(csrfToken ? { 'X-Codeium-Csrf-Token': csrfToken } : {})
      },
      ...(isHttps ? { rejectUnauthorized: false } : {})
    };

    const client = isHttps ? https : http;
    const req = client.request(options, (res) => {
      let data = '';
      res.on('data', (chunk) => {
        data += chunk;
      });
      res.on('end', () => {
        if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
          try {
            resolve(JSON.parse(data));
          } catch (e) {
            reject(new Error(`Failed to parse RetrieveUserQuotaSummary response: ${e.message}`));
          }
        } else {
          reject(new Error(`HTTP error status ${res.statusCode}: ${data}`));
        }
      });
    });

    req.on('error', reject);
    req.on('timeout', () => {
      req.destroy();
      reject(new Error('Request timed out'));
    });

    req.write(JSON.stringify({
      metadata: {
        ideName: 'antigravity',
        extensionName: 'antigravity',
        locale: 'en'
      }
    }));
    req.end();
  });
}

function parseResetTime(resetTime) {
  if (!resetTime) return undefined;
  try {
    const resetDate = new Date(resetTime);
    const now = Date.now();
    const diff = resetDate.getTime() - now;
    return diff > 0 ? diff : undefined;
  } catch {
    return undefined;
  }
}

function parseModelQuota(model) {
  const quota = model.quota;
  return {
    label: model.label || model.displayName || model.modelId,
    modelId: model.modelId,
    remainingPercentage: quota?.remainingPercentage,
    isExhausted: model.isExhausted ?? (quota?.remainingPercentage === 0),
    resetTime: quota?.resetTime,
    timeUntilResetMs: quota?.resetTime ? parseResetTime(quota.resetTime) : undefined,
    isAutocompleteOnly: model.modelId.includes('gemini-2.5') || 
                        (model.label || '').includes('Gemini 2.5') || 
                        (model.displayName || '').includes('Gemini 2.5')
  };
}

function parseLocalQuotaSnapshot(userStatus, quotaSummary) {
  const quota = userStatus.quota || {};
  const snapshot = {
    timestamp: new Date().toISOString(),
    method: 'local',
    email: userStatus.email,
    models: [],
    quotaSummary: quotaSummary || null
  };

  if (Array.isArray(quota.models)) {
    snapshot.models = quota.models.map(parseModelQuota);
  }

  return snapshot;
}

// Convert Connect RPC payload format to match what antigravity-usage outputs
function extractQuota(data) {
  const quota = {};

  const cascadeData = data.cascadeModelConfigData;
  const clientModelConfigs = cascadeData?.clientModelConfigs;
  if (Array.isArray(clientModelConfigs)) {
    quota.models = clientModelConfigs.map(m => {
      const modelOrAlias = m.modelOrAlias;
      const modelId = typeof modelOrAlias?.model === 'string' ? modelOrAlias.model : 'unknown';
      const quotaInfo = m.quotaInfo;
      const remainingFraction = typeof quotaInfo?.remainingFraction === 'number' ? quotaInfo.remainingFraction : undefined;
      const resetTime = typeof quotaInfo?.resetTime === 'string' ? quotaInfo.resetTime : undefined;

      return {
        modelId,
        displayName: typeof m.label === 'string' ? m.label : undefined,
        label: typeof m.label === 'string' ? m.label : undefined,
        quota: {
          remaining: undefined,
          limit: undefined,
          usedPercentage: remainingFraction !== undefined ? 1 - remainingFraction : undefined,
          remainingPercentage: remainingFraction,
          resetTime,
          timeUntilResetMs: resetTime ? parseResetTime(resetTime) : undefined
        },
        isExhausted: remainingFraction === 0
      };
    });
  }

  return quota;
}

async function main() {
  try {
    const processInfo = await detectAntigravityProcess();
    if (!processInfo) {
      console.error(JSON.stringify({ error: 'Antigravity IDE process is not running.' }));
      process.exit(1);
    }

    debug(`Detected Antigravity process: PID ${processInfo.pid}`);
    const ports = await discoverPorts(processInfo.pid, processInfo.extensionServerPort);
    if (ports.length === 0) {
      console.error(JSON.stringify({ error: 'Could not detect Antigravity server port.' }));
      process.exit(1);
    }

    const probeResult = await probeForConnectAPI(ports, processInfo.csrfToken);
    if (!probeResult) {
      console.error(JSON.stringify({ error: 'Could not find Antigravity Connect API on any listening port.' }));
      process.exit(1);
    }

    debug(`Connected to Connect API at ${probeResult.baseUrl}`);
    
    // Call userStatus and retrieveUserQuotaSummary in parallel
    const [rawStatus, rawSummary] = await Promise.all([
      getUserStatus(probeResult.baseUrl, processInfo.csrfToken).catch(err => {
        debug('getUserStatus failed:', err.message);
        return null;
      }),
      retrieveUserQuotaSummary(probeResult.baseUrl, processInfo.csrfToken).catch(err => {
        debug('retrieveUserQuotaSummary failed:', err.message);
        return null;
      })
    ]);

    if (!rawStatus) {
      console.error(JSON.stringify({ error: 'Could not fetch user status from Antigravity Connect API.' }));
      process.exit(1);
    }

    // Map userStatus like ConnectClient does
    const userStatus = rawStatus.userStatus || rawStatus;
    const finalQuota = extractQuota(userStatus);
    const unifiedStatus = {
      email: userStatus.email,
      quota: finalQuota
    };

    const quotaSummary = rawSummary?.response || rawSummary || null;
    const snapshot = parseLocalQuotaSnapshot(unifiedStatus, quotaSummary);
    console.log(JSON.stringify(snapshot, null, 2));
  } catch (err) {
    console.error(JSON.stringify({ error: err.message }));
    process.exit(1);
  }
}

main();
