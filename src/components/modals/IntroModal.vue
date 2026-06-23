<template>
  <BaseModal :show="showIntroModal" @close="closeIntroModal">
    <template #title>
      <i class="fa-solid fa-book-open mr-1" style="color: #6ee7b7;"></i> Hướng Dẫn: Lạc Việt Anh Workflow
    </template>

    <div class="modal-body scrollable intro-content">
      <div class="intro-header mb-3">
        <h3>🚀 Chào mừng đến với Aki Dev Sync</h3>
        <p>
          Đây là Command Center chuyên biệt được thiết kế để kết nối <strong>Máy Local</strong> và <strong>Máy Remote</strong>, giúp tối đa hóa sức mạnh của các AI Assistant trong khi vẫn đảm bảo tuyệt đối an toàn cho source code.
        </p>
      </div>

      <!-- Section: Nhu cầu khởi thủy -->
      <div class="alert-box primary mb-3">
        <h4 class="alert-title"><i class="fa-solid fa-bullseye"></i> Mục Đích Khởi Thủy (Tác giả)</h4>
        <p class="mb-1">
          Ứng dụng này sinh ra từ bài toán thực tế của Lạc Việt Anh:
        </p>
        <ul class="custom-list">
          <li>
            <strong>Local (Source of Truth):</strong> Nơi code an toàn, lưu trữ Git. Tại đây sử dụng <em>Antigravity Pro</em> cá nhân.
          </li>
          <li>
            <strong>Remote (AI Workspace):</strong> Nơi đẩy code lên để AI đọc hiểu toàn bộ project. Tại đây tận dụng <em>Claude MAX</em> (tài khoản dùng chung/được chia sẻ) chạy qua Terminal để sinh code hàng loạt một cách <strong>Bảo mật</strong>.
          </li>
        </ul>
      </div>

      <!-- Section: Use Cases Phổ Quát -->
      <div class="alert-box info mb-3">
        <h4 class="alert-title"><i class="fa-solid fa-earth-americas"></i> Use Cases Cho Mọi Lập Trình Viên</h4>
        <p class="mb-1">
          Dù bạn không dùng 2 loại AI khác nhau, workflow này vẫn cực kỳ hữu ích:
        </p>
        <ul class="custom-list">
          <li><strong>Máy Yếu ↔ Server Mạnh:</strong> Code nhẹ ở laptop cá nhân, đẩy việc tính toán/chạy AI/build nặng lên Server.</li>
          <li><strong>Bảo Mật Source Code:</strong> Làm việc ở máy công ty bị kiểm soát gắt gao? Chỉ giữ file cần thiết ở Local, xử lý core ở VPS cá nhân.</li>
          <li><strong>Làm Việc Đa Thiết Bị:</strong> Đồng bộ nhanh chóng luồng làm việc giữa PC, Laptop và Server mà không phải Commit rác lên GitHub.</li>
        </ul>
      </div>

      <!-- Section: Features -->
      <div class="features-grid">
        <div class="feature-card">
          <div class="feature-icon"><i class="fa-solid fa-code-commit"></i></div>
          <div class="feature-text">
            <strong>PUSH Thông Minh</strong>
            <span>Đẩy code và <code>.git/</code> sang Remote để AI và VSCode SSH hiểu ngữ cảnh.</span>
          </div>
        </div>
        
        <div class="feature-card">
          <div class="feature-icon" style="color: #f59e0b;"><i class="fa-solid fa-bolt"></i></div>
          <div class="feature-text">
            <strong>PUSH SPECIAL</strong>
            <span>Chỉ chọn đẩy các file Modified/Untracked sang Remote, tiết kiệm thời gian quét.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #3b82f6;"><i class="fa-solid fa-download"></i></div>
          <div class="feature-text">
            <strong>PULL Siêu Tốc</strong>
            <span>Rút ngay file AI vừa sửa từ Remote về Local để Review & Commit.</span>
          </div>
        </div>

        <div class="feature-card">
          <div class="feature-icon" style="color: #ec4899;"><i class="fa-solid fa-rotate"></i></div>
          <div class="feature-text">
            <strong>Background Reload</strong>
            <span>Giám sát Log & Git ngầm. Giao diện mượt mà không bao giờ bị treo (non-blocking).</span>
          </div>
        </div>
      </div>
    </div>

    <div class="modal-footer" style="justify-content: flex-end;">
      <button class="btn-tech btn-tech-primary" @click="closeIntroModal">
        <i class="fa-solid fa-check"></i> ĐÃ HIỂU
      </button>
    </div>
  </BaseModal>
</template>

<script setup>
import BaseModal from './BaseModal.vue';
import { useIntro } from '../../composables/useIntro';

const { showIntroModal, closeIntroModal } = useIntro();
</script>

<style scoped>
.intro-content {
  font-size: 14px;
  line-height: 1.5;
  color: #d1d5db;
}
.intro-header h3 {
  margin: 0 0 8px 0;
  color: #f3f4f6;
  font-size: 18px;
}
.intro-header p {
  margin: 0;
  color: #9ca3af;
}
.alert-box {
  padding: 14px;
  border-radius: 6px;
  background: rgba(5, 7, 12, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.05);
}
.alert-box.primary {
  background: rgba(110, 231, 183, 0.05);
  border-color: rgba(110, 231, 183, 0.2);
}
.alert-box.info {
  background: rgba(59, 130, 246, 0.05);
  border-color: rgba(59, 130, 246, 0.2);
}
.alert-title {
  margin: 0 0 10px 0;
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.alert-box.primary .alert-title {
  color: #6ee7b7;
}
.alert-box.info .alert-title {
  color: #60a5fa;
}
.custom-list {
  margin: 0;
  padding-left: 20px;
}
.custom-list li {
  margin-bottom: 6px;
}
.custom-list li:last-child {
  margin-bottom: 0;
}
.features-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.feature-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  background: rgba(5, 7, 12, 0.6);
  padding: 12px;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.05);
}
.feature-icon {
  font-size: 18px;
  color: #a78bfa;
  margin-top: 2px;
}
.feature-text strong {
  display: block;
  color: #e5e7eb;
  font-size: 13px;
  margin-bottom: 4px;
}
.feature-text span {
  font-size: 12px;
  color: #9ca3af;
}
.mb-1 { margin-bottom: 4px; }
.mb-2 { margin-bottom: 8px; }
.mb-3 { margin-bottom: 16px; }
</style>
