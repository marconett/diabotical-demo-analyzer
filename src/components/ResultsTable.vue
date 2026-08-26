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
          <th>Mode</th>
          <th>Map</th>
          <th>Player</th>
          <template v-if="criteriaColumns.length">
            <th v-for="column in criteriaColumns" :key="column.id" class="num criterion">
              <button class="sort" @click="sortBy(`criterion:${column.id}`)">
                {{ column.label }}{{ marker(`criterion:${column.id}`) }}
              </button>
            </th>
          </template>
          <template v-else>
            <th v-if="hasWeapon">Weapon</th>
            <th class="num">
              <button class="sort" @click="sortBy('damage')">Damage{{ marker('damage') }}</button>
            </th>
            <th class="num">
              <button class="sort" @click="sortBy('frags')">Frags{{ marker('frags') }}</button>
            </th>
            <th v-if="hasSpeed" class="num">
              <button class="sort" @click="sortBy('speed')">Max speed{{ marker('speed') }}</button>
            </th>
            <th v-if="hasSpeed" class="num">Time ≥ min</th>
            <th v-if="hasAccuracy" class="num">
              <button class="sort" @click="sortBy('accuracy')">
                Accuracy{{ marker('accuracy') }}
              </button>
            </th>
          </template>
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
          <template v-if="criteriaColumns.length">
            <td v-for="column in criteriaColumns" :key="column.id" class="num">
              {{ r.criteria[column.id]?.display ?? '' }}
            </td>
          </template>
          <template v-else>
            <td v-if="hasWeapon">{{ weaponName(r.weapon) }}</td>
            <td class="num">{{ r.damage ?? '' }}</td>
            <td class="num">{{ r.frags ?? '' }}</td>
            <td v-if="hasSpeed" class="num">{{ r.speed ?? '' }}</td>
            <td v-if="hasSpeed" class="num">{{ fmtDuration(r.speedDuration) }}</td>
            <td v-if="hasAccuracy" class="num">
              {{ r.accuracy === null ? '' : `${r.accuracy.toFixed(1)}%` }}
            </td>
          </template>
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
import type { RunReport, SceneMetric } from '../lib/types';
import { weaponName } from '../lib/weapons';

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
  weapon: number | null;
  speed: number | null;
  speedDuration: number | null;
  accuracy: number | null;
  criteria: Record<string, SceneMetric>;
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
      weapon: s.weapon,
      speed: s.speed,
      speedDuration: s.speedDuration,
      accuracy: s.accuracy,
      criteria: Object.fromEntries((s.criteria ?? []).map((metric) => [metric.ruleId, metric])),
    }));
  }),
);

const criteriaColumns = computed(() => {
  const columns = new Map<string, string>();
  for (const row of rows.value) {
    for (const metric of Object.values(row.criteria)) columns.set(metric.ruleId, metric.label);
  }
  return [...columns].map(([id, label]) => ({ id, label }));
});

const sortKey = ref('time');
const sortDirection = ref<1 | -1>(1);

function sortBy(key: string) {
  if (sortKey.value === key) sortDirection.value = sortDirection.value === 1 ? -1 : 1;
  else {
    sortKey.value = key;
    sortDirection.value = key === 'time' || key === 'demo' ? 1 : -1;
  }
}

function marker(key: string): string {
  return sortKey.value === key ? (sortDirection.value === 1 ? ' ▲' : ' ▼') : '';
}

function sortValue(row: Row, key: string): number | string {
  if (key === 'time') return row.start;
  if (key === 'demo') return row.fileName.toLocaleLowerCase();
  if (key.startsWith('criterion:')) return row.criteria[key.slice(10)]?.value ?? -Infinity;
  return row[key as 'damage' | 'frags' | 'speed' | 'accuracy'] ?? -Infinity;
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

const hasWeapon = computed(() => rows.value.some((row) => row.weapon !== null));
const hasSpeed = computed(() => rows.value.some((row) => row.speed !== null));
const hasAccuracy = computed(() => rows.value.some((row) => row.accuracy !== null));

const withoutScenes = computed(
  () => props.report.demos.filter((d) => (d.sceneCount ?? 0) === 0).length,
);

function fmtDuration(seconds: number | null): string {
  return seconds === null ? '' : `${seconds.toFixed(1)}s`;
}

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

th.criterion {
  max-width: 180px;
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
