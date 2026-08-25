<template>
  <div class="tags" :class="{ focused }" @click="focusInput">
    <span v-for="(tag, i) in modelValue" :key="tag" class="chip">
      {{ tag }}
      <button
        type="button"
        class="chip-remove"
        :aria-label="`Remove ${tag}`"
        @click.stop="remove(i)"
      >
        ×
      </button>
    </span>
    <div class="entry">
      <input
        ref="inputEl"
        v-model="query"
        type="text"
        :placeholder="modelValue.length ? '' : placeholder"
        :disabled="disabled"
        autocomplete="off"
        spellcheck="false"
        @focus="onFocus"
        @blur="onBlur"
        @keydown="onKeydown"
      />
      <ul v-if="open && filtered.length" class="popover" role="listbox">
        <li
          v-for="(s, i) in filtered"
          :key="s.name"
          role="option"
          :aria-selected="i === highlight"
          :class="{ active: i === highlight }"
          @mousedown.prevent="add(s.name)"
          @mousemove="highlight = i"
        >
          <span class="name">{{ s.name }}</span>
          <span class="count">{{ s.demos }} demo{{ s.demos === 1 ? '' : 's' }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { PlayerHit } from '../lib/types';

const props = withDefaults(
  defineProps<{
    modelValue: string[];
    suggestions: PlayerHit[];
    placeholder?: string;
    disabled?: boolean;
  }>(),
  { placeholder: '', disabled: false },
);
const emit = defineEmits<{ 'update:modelValue': [value: string[]] }>();

const inputEl = ref<HTMLInputElement | null>(null);
const query = ref('');
const focused = ref(false);
const open = ref(false);
const highlight = ref(0);

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  const chosen = new Set(props.modelValue);
  return props.suggestions
    .filter((s) => !chosen.has(s.name) && (!q || s.name.toLowerCase().includes(q)))
    .slice(0, 40);
});

watch(filtered, () => {
  if (highlight.value >= filtered.value.length) highlight.value = 0;
});

function focusInput() {
  inputEl.value?.focus();
}

function onFocus() {
  focused.value = true;
  open.value = true;
}

function onBlur() {
  focused.value = false;
  open.value = false;
  commitTyped();
}

function commitTyped() {
  const raw = query.value.trim();
  if (raw) add(raw);
}

function add(name: string) {
  const n = name.trim();
  if (n && !props.modelValue.includes(n)) emit('update:modelValue', [...props.modelValue, n]);
  query.value = '';
  highlight.value = 0;
}

function remove(i: number) {
  emit(
    'update:modelValue',
    props.modelValue.filter((_, j) => j !== i),
  );
}

function onKeydown(e: KeyboardEvent) {
  switch (e.key) {
    case 'Enter':
    case ',':
      e.preventDefault();
      if (open.value && filtered.value.length && (highlight.value > 0 || !query.value.trim())) {
        add(filtered.value[highlight.value].name);
      } else if (open.value && filtered.value.length && query.value.trim()) {
        // A typed prefix picks the highlighted suggestion; exact free text still wins.
        const exact = filtered.value.find((s) => s.name === query.value.trim());
        add(exact ? exact.name : highlight.value === 0 ? filtered.value[0].name : query.value);
      } else {
        commitTyped();
      }
      open.value = true;
      break;
    case 'Backspace':
      if (!query.value && props.modelValue.length) remove(props.modelValue.length - 1);
      break;
    case 'ArrowDown':
      e.preventDefault();
      open.value = true;
      if (filtered.value.length) highlight.value = (highlight.value + 1) % filtered.value.length;
      break;
    case 'ArrowUp':
      e.preventDefault();
      if (filtered.value.length)
        highlight.value = (highlight.value - 1 + filtered.value.length) % filtered.value.length;
      break;
    case 'Escape':
      open.value = false;
      break;
    default:
      open.value = true;
  }
}
</script>

<style scoped>
.tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  min-height: 29px;
  padding: 3px 6px;
  background: var(--inputBg);
  border: 1px solid var(--borderColor);
  border-radius: 5px;
  cursor: text;
}

.tags.focused {
  border-color: var(--accentColor);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accentColor) 25%, transparent);
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 4px 1px 8px;
  font-size: 13px;
  background: var(--shadeColor);
  border: 1px solid var(--borderColor);
  border-radius: 4px;
  user-select: text;
}

.chip-remove {
  padding: 0 4px;
  font-size: 14px;
  line-height: 1;
  background: transparent;
  border: none;
  color: var(--mutedColor);
}

.chip-remove:hover {
  color: var(--dangerColor);
  background: transparent;
}

.entry {
  position: relative;
  flex: 1;
  min-width: 140px;
}

.entry input {
  width: 100%;
  border: none;
  background: transparent;
  padding: 2px 2px;
}

.entry input:focus {
  outline: none;
  box-shadow: none;
}

.popover {
  position: absolute;
  z-index: 10;
  top: calc(100% + 6px);
  left: 0;
  min-width: 260px;
  max-height: 280px;
  overflow-y: auto;
  margin: 0;
  padding: 4px;
  list-style: none;
  background: var(--bgColor);
  border: 1px solid var(--borderColor);
  border-radius: 6px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.18);
}

.popover li {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 4px 8px;
  border-radius: 4px;
  font-size: 13px;
}

.popover li.active {
  background: var(--accentBg);
  color: var(--accentFg);
}

.popover .count {
  color: var(--mutedColor);
  font-size: 11.5px;
  font-variant-numeric: tabular-nums;
}

.popover li.active .count {
  color: var(--accentFg);
  opacity: 0.8;
}
</style>
