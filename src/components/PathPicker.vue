<template>
  <div class="picker">
    <div class="row">
      <input
        v-model="typed"
        type="text"
        placeholder="Demo file or folder"
        spellcheck="false"
        :disabled="disabled"
        @keydown.enter="submitTyped"
      />
      <button :disabled="disabled" @click="pickFiles">File(s)…</button>
      <button :disabled="disabled" @click="pickFolder">Folder…</button>
    </div>
    <div class="status">
      <template v-if="list">
        <strong>{{ list.files.length }}</strong> demo{{ list.files.length === 1 ? '' : 's' }},
        {{ fmtBytes(list.totalBytes) }}
      </template>
      <span v-else-if="listing">Listing…</span>
      <span v-else class="hint"
        >Accepts .rbr (client) and .srd (server) recordings; folders are searched recursively.</span
      >
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { fmtBytes } from '../lib/format';
import type { DemoList } from '../lib/types';

const props = defineProps<{
  paths: string[];
  list: DemoList | null;
  listing: boolean;
  disabled: boolean;
}>();
const emit = defineEmits<{ select: [paths: string[]] }>();

const typed = ref('');
watch(
  () => props.paths,
  (p) => {
    typed.value = p.length === 1 ? p[0] : p.join('; ');
  },
  { immediate: true },
);

function submitTyped() {
  const parts = typed.value
    .split(';')
    .map((s) => s.trim())
    .filter(Boolean);
  if (parts.length) emit('select', parts);
}

async function pickFiles() {
  const picked = await open({
    title: 'Choose demos',
    multiple: true,
    filters: [{ name: 'Diabotical demos', extensions: ['rbr', 'srd'] }],
  });
  if (!picked) return;
  emit('select', Array.isArray(picked) ? picked : [picked]);
}

async function pickFolder() {
  const picked = await open({ title: 'Choose a demo folder', directory: true, multiple: false });
  if (typeof picked === 'string') emit('select', [picked]);
}
</script>

<style scoped>
.picker {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.row {
  display: flex;
  gap: 6px;
}

.row input {
  flex: 1;
  min-width: 0;
}

.status {
  font-size: 12px;
  color: var(--mutedColor);
  min-height: 18px;
}

.hint {
  color: var(--mutedColor);
}
</style>
