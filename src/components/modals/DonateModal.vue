<template>
  <BaseModal :show="show" @close="$emit('close')" container-style="width: 340px; max-width: calc(100vw - 32px);">
    <template #title>
      <i class="fa-solid fa-heart"></i> Donate to AkiDevSync
    </template>

    <div class="modal-body">
      <p class="donate-blurb">If you find this tool helpful, consider supporting its development!</p>

      <div class="qr-row">
        <span class="qr-label">PayPal</span>
        <img src="/QR-AkiTao-PayPal.png" alt="PayPal donation QR" class="qr-img" />
      </div>

      <div class="qr-row">
        <span class="qr-label">MoMo</span>
        <img src="/QR-Aki.MOMO.jpg" alt="MoMo donation QR" class="qr-img" />
      </div>

      <a href="#" @click.prevent="openLink(BANK_QR_URL)" class="bank-link">
        <i class="fa-solid fa-building-columns"></i> Prefer bank transfer? Open VietQR
      </a>
    </div>
  </BaseModal>
</template>

<script setup>
import { invoke } from '../../utils/tauri';
import BaseModal from './BaseModal.vue';

defineProps({ show: { type: Boolean, default: false } });
defineEmits(['close']);

const BANK_QR_URL = 'https://app.akinet.me/en/qr-bank/?bank=970422&acc=0869297957&tpl=print&amount=0&info=Donate+AkiDevSync&name=LacVietAnh&view=1';

function openLink(url) {
  invoke('macos_open', { args: [url] }).catch(console.error);
}
</script>

<style scoped>
.modal-body {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
}

.donate-blurb {
  font-size: 11px;
  color: #94a3b8;
  margin: 0 0 4px;
  text-align: center;
}

.qr-row {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  width: 100%;
}

.qr-label {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: #64748b;
}

.qr-img {
  width: 200px;
  height: 200px;
  object-fit: contain;
  border-radius: 8px;
  background: #fff;
  padding: 6px;
}

.bank-link {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: #64748b;
  text-decoration: none;
  margin-top: 4px;
}

.bank-link:hover {
  color: #a5f3fc;
}
</style>
