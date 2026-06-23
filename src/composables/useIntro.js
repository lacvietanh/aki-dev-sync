import { ref } from "vue";

const showIntroModal = ref(false);

export function useIntro() {
  function openIntroModal() {
    showIntroModal.value = true;
  }

  function closeIntroModal() {
    showIntroModal.value = false;
  }

  return {
    showIntroModal,
    openIntroModal,
    closeIntroModal
  };
}
