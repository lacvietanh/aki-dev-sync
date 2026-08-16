/**
 * scripts/test/monitor-ag-proxy.js
 * 
 * Purpose:
 * Standalone verification script that continuously polls the local Antigravity IDE
 * Language Server quota via get-antigravity-usage.sh every 5 seconds.
 *
 * Used for testing and monitoring connection stability in isolation without launching
 * the Tauri desktop UI.
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

// Formatting helpers
const formatTime = () => new Date().toLocaleTimeString('vi-VN', { hour12: false });
const logSuccess = (msg) => console.log(`\x1b[32m[${formatTime()}] 🟢 OK: \x1b[0m${msg}`);
const logError = (msg) => console.log(`\x1b[31m[${formatTime()}] 🔴 FAIL: \x1b[0m${msg}`);
const logInfo = (msg) => console.log(`\x1b[34m[${formatTime()}] 🔵 INFO: \x1b[0m${msg}`);

// Probe (docs/research/ghost-files-and-audit-method-jul30.md): .sh outputs |||FRAME||| text parsed in Rust (src-tauri/src/agent_usage/antigravity_payload.rs); checkProxy JSON.parse reports framing.
const COMMAND = 'sh scripts/get-antigravity-usage.sh';
const INTERVAL_MS = 5000;

console.log(`\x1b[36m========================================`);
console.log(`🚀 Bắt đầu giám sát Antigravity Proxy`);
console.log(`⏳ Chu kỳ: ${INTERVAL_MS / 1000}s`);
console.log(`💻 Lệnh: ${COMMAND}`);
console.log(`========================================\x1b[0m\n`);

async function checkProxy() {
  try {
    const { stdout, stderr } = await execAsync(COMMAND);
    
    // In lỗi nếu stderr có nội dung nhưng tiến trình không crash.
    if (stderr && stderr.trim().length > 0) {
      logError(stderr.trim());
      return;
    }

    try {
      const data = JSON.parse(stdout);
      
      // Bóc tách dữ liệu để in tóm tắt.
      const email = data.email || 'Unknown';
      const geminiModel = data.models?.find(m => m.label.toLowerCase().includes('gemini'));
      const usedPct = geminiModel 
        ? ((1 - geminiModel.remainingPercentage) * 100).toFixed(1) + '%' 
        : 'N/A';

      logSuccess(`Proxy phản hồi tốt! Account: ${email} | Gemini Used: ${usedPct}`);
    } catch (parseErr) {
      logError(`Lỗi parse JSON. Output thô: ${stdout.substring(0, 100)}...`);
    }

  } catch (error) {
    // IDE ngủ đông/lỗi trả về non-zero exit code; thông điệp nằm ở error.stderr hoặc error.message.
    const errorMsg = error.stderr ? error.stderr.trim() : error.message.split('\n')[0];
    logError(`Mất kết nối Proxy: ${errorMsg}`);
  }
}

// Chạy ngay lần đầu tiên.
checkProxy();

// Polling định kỳ mỗi 5s.
setInterval(checkProxy, INTERVAL_MS);
