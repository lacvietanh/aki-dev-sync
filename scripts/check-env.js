import os from 'node:os';

const isLinux = os.platform() === 'linux';
const isSSH = process.env.SSH_CONNECTION || process.env.SSH_CLIENT || process.env.SSH_TTY;

if (isLinux) {
  console.log('\n\x1b[33m%s\x1b[0m', '=============================================================================');
  console.log('\x1b[33m%s\x1b[0m', '⚠️  CẢNH BÁO: Hệ điều hành Linux được phát hiện.');
  if (isSSH) {
    console.log('\x1b[31m%s\x1b[0m', 'Bạn đang chạy qua phiên kết nối SSH Remote!');
    console.log('\x1b[31m%s\x1b[0m', 'App Tauri (Desktop) yêu cầu môi trường đồ họa (GUI) để vẽ cửa sổ.');
    console.log('\x1b[31m%s\x1b[0m', 'Nếu đây là một máy chủ headless, quá trình chạy Tauri Window có thể sẽ lỗi!');
    console.log('\x1b[31m%s\x1b[0m', 'Lưu ý: Bạn vẫn xem được giao diện Frontend qua Web Browser (Vite dev port).');
  } else {
    console.log('\x1b[33m%s\x1b[0m', 'Tauri yêu cầu môi trường Desktop (X11/Wayland) với thư viện WebKit2GTK.');
  }
  console.log('\x1b[33m%s\x1b[0m', '=============================================================================\n');
}
