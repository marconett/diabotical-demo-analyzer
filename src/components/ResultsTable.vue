<template>
  <div class="results">
    <div class="howto" role="note">
      <span class="howto-icon" aria-hidden="true">▶</span>
      <span>
        <strong>Copy command</strong> puts a <code>/play</code> line on your clipboard; paste it
        into the Diabotical console to open that demo 5 seconds before the scene.
      </span>
    </div>

    <div class="summary">
      <strong> {{ report.totalScenes }} scene{{ report.totalScenes === 1 ? '' : 's' }} </strong>
      across {{ report.demosWithPlayer }}/{{ report.demoCount }} demo{{
        report.demoCount === 1 ? '' : 's'
      }}
      <template v-if="report.cancelled"> · cancelled</template>
    </div>

    <table v-if="rows.length">
      <thead>
        <tr>
          <th class="action"></th>
          <th>
            <button class="sort" @click="sortBy('demo')">Demo{{ marker('demo') }}</button>
          </th>
          <th class="num">
            <button class="sort" @click="sortBy('time')">Time{{ marker('time') }}</button>
          </th>
          <th>
            <button class="sort" @click="sortBy('mode')">Mode{{ marker('mode') }}</button>
          </th>
          <th>
            <button class="sort" @click="sortBy('map')">Map{{ marker('map') }}</button>
          </th>
          <th>
            <button class="sort" @click="sortBy('player')">Player{{ marker('player') }}</button>
          </th>
          <th class="num">
            <button class="sort" @click="sortBy('damage')">Damage{{ marker('damage') }}</button>
          </th>
          <th class="num">
            <button class="sort" @click="sortBy('frags')">Frags{{ marker('frags') }}</button>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in sortedRows" :key="r.key" :title="r.command">
          <td class="action">
            <button class="small copy" :class="{ done: copiedKey === r.key }" @click="copy(r)">
              {{ copiedKey === r.key ? 'Copied' : 'Copy command' }}
            </button>
          </td>
          <td>{{ r.fileName }}</td>
          <td class="num time">{{ r.time }}</td>
          <td>{{ r.mode }}</td>
          <td>{{ r.map }}</td>
          <td>{{ r.player }}</td>
          <td class="num">{{ r.damage ?? '' }}</td>
          <td class="num">{{ r.frags ?? '' }}</td>
        </tr>
      </tbody>
    </table>

    <p v-if="withoutScenes" class="quiet">
      {{ withoutScenes }} demo{{ withoutScenes === 1 ? '' : 's' }} without a scene
    </p>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import type { RunReport } from '../lib/types';

const props = defineProps<{ report: RunReport }>();

interface Row {
  key: string;
  command: string;
  fileName: string;
  time: string;
  start: number;
  mode: string;
  map: string;
  player: string;
  damage: number | null;
  frags: number | null;
}

// Seconds before the scene at which playback starts, so the run-up is visible.
const LEAD_SECONDS = 5;

const copiedKey = ref<string | null>(null);
let copiedTimer: ReturnType<typeof setTimeout> | undefined;

const rows = computed<Row[]>(() =>
  props.report.demos.flatMap((d) => {
    const demoName = d.fileName.replace(/\.[^.]+$/, '');
    return d.scenes.map((s, i) => ({
      key: `${d.path}#${i}`,
      command: `/play ${demoName} ${Math.max(0, Math.floor(s.start - LEAD_SECONDS))}`,
      fileName: d.fileName,
      time: s.clock.replace(/^\[|\]$/g, ''),
      start: s.start,
      mode: d.meta?.gameMode ?? '',
      map: d.meta?.mapName ?? '',
      player: s.player,
      damage: s.damage,
      frags: s.frags,
    }));
  }),
);

type SortKey = 'demo' | 'time' | 'mode' | 'map' | 'player' | 'damage' | 'frags';

const sortKey = ref<SortKey>('time');
const sortDirection = ref<1 | -1>(1);

function sortBy(key: SortKey) {
  if (sortKey.value === key) sortDirection.value = sortDirection.value === 1 ? -1 : 1;
  else {
    sortKey.value = key;
    sortDirection.value = key === 'damage' || key === 'frags' ? -1 : 1;
  }
}

function marker(key: SortKey): string {
  return sortKey.value === key ? (sortDirection.value === 1 ? ' ▲' : ' ▼') : '';
}

function sortValue(row: Row, key: SortKey): number | string {
  if (key === 'time') return row.start;
  if (key === 'demo') return row.fileName.toLocaleLowerCase();
  if (key === 'damage' || key === 'frags') return row[key] ?? -Infinity;
  return row[key].toLocaleLowerCase();
}

const sortedRows = computed(() =>
  [...rows.value].sort((a, b) => {
    const av = sortValue(a, sortKey.value);
    const bv = sortValue(b, sortKey.value);
    const order =
      typeof av === 'string' && typeof bv === 'string'
        ? av.localeCompare(bv)
        : Number(av) - Number(bv);
    return order * sortDirection.value || a.start - b.start || a.key.localeCompare(b.key);
  }),
);

const withoutScenes = computed(
  () => props.report.demos.filter((d) => (d.sceneCount ?? 0) === 0).length,
);

async function copy(r: Row) {
  try {
    await navigator.clipboard.writeText(r.command);
  } catch {
    // The clipboard API is unavailable without a secure context; the command
    // is still readable as the row's tooltip.
    return;
  }
  copiedKey.value = r.key;
  clearTimeout(copiedTimer);
  copiedTimer = setTimeout(() => (copiedKey.value = null), 1500);
}
</script>

<style scoped>
.results {
  display: flex;
  flex-direction: column;
  gap: 12px;
  font-size: 14.5px;
}

.howto {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border: 1px solid color-mix(in srgb, var(--accentColor) 45%, var(--borderColor));
  border-left: 4px solid var(--accentColor);
  border-radius: 6px;
  background: color-mix(in srgb, var(--accentColor) 9%, var(--bgColor));
  line-height: 1.45;
}

.howto-icon {
  color: var(--accentColor);
  font-size: 12px;
}

.howto code {
  padding: 0 4px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--accentColor) 14%, transparent);
}

.summary {
  display: flex;
  align-items: baseline;
  gap: 6px;
  flex-wrap: wrap;
  color: var(--mutedColor);
}

.summary strong {
  color: var(--fgColor);
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  padding: 6px 8px;
  text-align: left;
  border-bottom: 1px solid var(--borderColor);
  white-space: nowrap;
}

th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--bgColor);
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--mutedColor);
}

button.sort {
  padding: 0;
  border: 0;
  color: inherit;
  background: transparent;
  font: inherit;
  text-transform: inherit;
  letter-spacing: inherit;
  white-space: normal;
  text-align: inherit;
  cursor: pointer;
}

button.sort:hover {
  color: var(--fgColor);
}

td {
  user-select: text;
}

tbody tr:hover td {
  background: color-mix(in srgb, var(--shadeColor) 60%, transparent);
}

th.action,
td.action {
  width: 1px;
  padding-left: 4px;
}

.num {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.time {
  font-family: ui-monospace, 'SF Mono', SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.92em;
  color: var(--accentColor);
}

button.copy {
  padding: 3px 10px;
  font-size: 13px;
  white-space: nowrap;
}

button.copy.done {
  color: var(--successColor);
  border-color: var(--successColor);
}

.quiet {
  margin: 0;
  color: var(--mutedColor);
}
</style>
