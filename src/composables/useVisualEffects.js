import { ref } from 'vue';

const STORAGE_KEY = 'aki-devsync-glass';
const ROOT_CLASS = 'fx-glass';

function readStored() {
  return localStorage.getItem(STORAGE_KEY) === '1';
}

function applyRootClass(on) {
  document.documentElement.classList.toggle(ROOT_CLASS, on);
}

export const glassEnabled = ref(readStored());

export function setGlassEnabled(on) {
  glassEnabled.value = on;
  localStorage.setItem(STORAGE_KEY, on ? '1' : '0');
  applyRootClass(on);
}

applyRootClass(glassEnabled.value);
