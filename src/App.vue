<template>
  <div class="shell" :class="{ dragging }">
    <header class="chrome">
      <span class="brand" role="img" aria-label="Diabotical Demo Analyzer"></span>
      <div class="title">
        <strong>Diabotical Demo Analyzer</strong>
        <span class="tagline">Find a player's standout moments in Diabotical demos</span>
      </div>
      <span class="spacer"></span>
      <span
        v-if="cache"
        class="cache-info"
        title="Extracted demo data is cached so re-runs with other parameters are instant"
      >
        cache: {{ cache.entries }} demo{{ cache.entries === 1 ? '' : 's' }},
        {{ fmtBytes(cache.bytes) }}
      </span>
      <button :disabled="busy !== 'idle' || !cache?.entries" @click="onClearCache">
        Clear cache
      </button>
    </header>

    <main class="content">
      <div class="column form">
        <Panel title="Input">
          <PathPicker
            :paths="inputs"
            :list="demoList"
            :listing="listing"
            :disabled="busy === 'run'"
            @select="selectPaths"
          />
        </Panel>

        <Panel title="Players">
          <div class="field">
            <TagInput
              v-model="params.players"
              :suggestions="discovered"
              placeholder="Player name (type or pick from the list; Enter adds)"
              :disabled="busy === 'run'"
            />
            <div class="field-row">
              <span v-if="discoverInfo" class="hint">{{ discoverInfo }}</span>
              <span v-else class="hint"
                >Names are read from the start of each demo; late joiners can be typed by
                hand.</span
              >
            </div>
          </div>
        </Panel>

        <Panel title="Criteria">
          <CriteriaBuilder
            v-if="params.ruleQuery"
            v-model="params.ruleQuery"
            :disabled="busy === 'run'"
          />
          <div class="window-row">
            <label for="window">Time window</label>
            <div class="with-unit">
              <input
                id="window"
                v-model.number="windowInput"
                type="number"
                min="0"
                step="0.5"
                placeholder="seconds"
                :disabled="busy === 'run'"
              />
              <span class="hint">seconds; required with metric thresholds</span>
            </div>
          </div>

          <p class="criteria-note">
            Speed is the server-reported horizontal velocity, so teleporters do not create false
            spikes. Weapon damage is available for the recording POV. Accuracy is the game's
            cumulative value at the scene, not accuracy within the window.
          </p>

          <div class="actions">
            <button class="primary" :disabled="!canRun" @click="run">Run</button>
            <button v-if="busy !== 'idle'" class="danger" @click="onCancel">Cancel</button>
            <span v-if="validation" class="validation">{{ validation }}</span>
          </div>
        </Panel>

        <Panel v-if="progress && busy !== 'idle'">
          <ProgressBar :progress="progress" />
        </Panel>

        <div v-if="error" class="error-banner">
          <strong>Error:</strong> {{ error }}
          <button class="small" @click="error = null">Dismiss</button>
        </div>
      </div>

      <div class="column results">
        <Panel title="Results" class="results-panel">
          <ResultsTable v-if="report" :report="report" />
          <p v-else class="empty">
            Pick a demo file or folder, choose players and criteria, then run.
          </p>
        </Panel>
      </div>
    </main>

    <footer class="statusbar">
      <span class="status-cell">
        <span class="dot" :class="{ on: busy !== 'idle' }"></span>
        {{ statusText }}
      </span>
      <span class="spacer"></span>
      <span v-if="demoList" class="status-cell"
        >{{ demoList.files.length }} demos · {{ fmtBytes(demoList.totalBytes) }}</span
      >
      <span v-if="report" class="status-cell">
        last run {{ fmtSeconds(report.elapsedMs) }} · {{ report.cacheHits }}/{{ report.demoCount }}
        from cache
      </span>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import Panel from './components/Panel.vue';
import PathPicker from './components/PathPicker.vue';
import TagInput from './components/TagInput.vue';
import ProgressBar from './components/ProgressBar.vue';
import ResultsTable from './components/ResultsTable.vue';
import CriteriaBuilder from './components/CriteriaBuilder.vue';
import * as api from './lib/api';
import { fmtBytes, fmtSeconds } from './lib/format';
import { initTheme } from './lib/theme';
import { validateParams } from './lib/validate';
import type { CacheStats, DemoList, Params, PlayerHit, Progress, RunReport } from './lib/types';

type Busy = 'idle' | 'discover' | 'run';

const inputs = ref<string[]>([]);
const demoList = ref<DemoList | null>(null);
const listing = ref(false);
const discovered = ref<PlayerHit[]>([]);
const discoverInfo = ref('');
const params = reactive<Params>({
  players: [],
  damage: 500,
  frags: 4,
  weapon: null,
  speed: null,
  speedDuration: null,
  accuracy: null,
  condition: 'AND',
  win: false,
  window: 15,
  ruleQuery: {
    condition: 'AND',
    groups: [
      {
        id: 'group-1',
        condition: 'AND',
        rules: [
          { id: 'damage-1', kind: 'damage', minimum: 500, weapon: null },
          { id: 'frags-1', kind: 'frags', minimum: 4, weapon: null },
        ],
      },
    ],
  },
});
const windowInput = ref<number | ''>(15);
const busy = ref<Busy>('idle');
const progress = ref<Progress | null>(null);
const report = ref<RunReport | null>(null);
const error = ref<string | null>(null);
const cache = ref<CacheStats | null>(null);
const dragging = ref(false);
watch(windowInput, (v) => (params.window = typeof v === 'number' && Number.isFinite(v) ? v : null));

const validation = computed(() =>
  demoList.value?.files.length ? validateParams(params) : 'choose a demo file or folder first',
);
const canRun = computed(() => busy.value === 'idle' && validation.value === null);

const statusText = computed(() => {
  if (busy.value === 'discover') return 'Listing players…';
  if (busy.value === 'run') return 'Scanning…';
  return 'Idle';
});

const demoPaths = computed(() => demoList.value?.files.map((f) => f.path) ?? []);

async function refreshCache() {
  try {
    cache.value = await api.cacheStats();
  } catch {
    cache.value = null;
  }
}

// Choosing a new input while the player list is still being read cancels
// that pass and waits for it to wind down.
async function settleDiscover() {
  if (busy.value !== 'discover') return;
  await api.cancel();
  for (let i = 0; i < 100 && (busy.value as Busy) !== 'idle'; i++) {
    await new Promise((r) => setTimeout(r, 50));
  }
}

async function selectPaths(paths: string[]) {
  if (busy.value === 'run') return;
  await settleDiscover();
  if (busy.value !== 'idle') return;
  inputs.value = paths;
  listing.value = true;
  error.value = null;
  try {
    demoList.value = await api.listDemos(paths);
    discovered.value = [];
    discoverInfo.value = '';
    if (demoList.value.files.length) await discover();
  } catch (e) {
    demoList.value = null;
    error.value = String(e);
  } finally {
    listing.value = false;
  }
}

async function discover() {
  if (busy.value !== 'idle' || !demoPaths.value.length) return;
  busy.value = 'discover';
  progress.value = null;
  try {
    const result = await api.discoverPlayers(demoPaths.value, (p) => (progress.value = p));
    discovered.value = result.players;
    const parts = [`${result.players.length} players in ${fmtSeconds(result.elapsedMs)}`];
    if (result.fromCache) parts.push(`${result.fromCache} demos from cache`);
    if (result.errors.length) parts.push(`${result.errors.length} unreadable`);
    if (result.cancelled) parts.push('cancelled');
    discoverInfo.value = parts.join(' · ');
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = 'idle';
    await refreshCache();
  }
}

async function run() {
  if (!canRun.value) return;
  busy.value = 'run';
  progress.value = null;
  error.value = null;
  const sent: Params = {
    ...params,
    players: [...params.players],
    ruleQuery: params.ruleQuery ? JSON.parse(JSON.stringify(params.ruleQuery)) : null,
  };
  try {
    report.value = await api.runSceneFinder(demoPaths.value, sent, (p) => (progress.value = p));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = 'idle';
    await refreshCache();
  }
  // A full run leaves complete extracts in the cache, so the name list now
  // covers players who joined mid-match; re-reading it costs nothing.
  await discover();
}

async function onCancel() {
  await api.cancel();
}

async function onClearCache() {
  try {
    await api.clearCache();
  } catch (e) {
    error.value = String(e);
  }
  await refreshCache();
}

onMounted(async () => {
  initTheme();
  await refreshCache();
  await getCurrentWebview().onDragDropEvent((event) => {
    const t = event.payload.type;
    if (t === 'enter' || t === 'over') dragging.value = true;
    else if (t === 'leave') dragging.value = false;
    else if (t === 'drop') {
      dragging.value = false;
      void selectPaths(event.payload.paths);
    }
  });
});
</script>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.shell.dragging::after {
  content: 'Drop demos or a folder';
  position: fixed;
  inset: 8px;
  display: grid;
  place-content: center;
  font-size: 18px;
  color: var(--accentColor);
  background: color-mix(in srgb, var(--accentColor) 8%, var(--bgColor));
  border: 2px dashed var(--accentColor);
  border-radius: 10px;
  pointer-events: none;
  z-index: 50;
}

/* Chrome bar: the app mark, name and tagline left; cache controls right. */
.chrome {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 14px;
  background: var(--chromeBg);
  border-bottom: 1px solid var(--borderColor);
}

/* The app mark, masked so it inherits the theme's foreground instead of
   shipping a light and a dark bitmap. */
.brand {
  height: 30px;
  aspect-ratio: 550.55 / 437.12;
  flex-shrink: 0;
  background: var(--fgColor);
  -webkit-mask: url('./assets/logo.svg') center / contain no-repeat;
  mask: url('./assets/logo.svg') center / contain no-repeat;
}

.title {
  display: flex;
  flex-direction: column;
  line-height: 1.25;
}

.tagline {
  font-size: 11.5px;
  color: var(--mutedColor);
}

.spacer {
  flex: 1;
}

.cache-info {
  font-size: 11.5px;
  color: var(--mutedColor);
  font-variant-numeric: tabular-nums;
}

.content {
  flex: 1;
  display: flex;
  gap: 14px;
  padding: 14px;
  min-height: 0;
  overflow: hidden;
}

.column {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

.column.form {
  width: 440px;
  flex-shrink: 0;
  overflow-y: auto;
}

.column.results {
  flex: 1;
  min-height: 0;
}

.results-panel {
  flex: 1;
  min-height: 0;
}

.results-panel :deep(.panel-body) {
  overflow: auto;
  height: 100%;
}

.results-panel :deep(.panel) {
  height: 100%;
}

.empty {
  margin: 0;
  color: var(--mutedColor);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.hint {
  font-size: 11.5px;
  color: var(--mutedColor);
}

.grid {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 8px 12px;
  align-items: center;
}

.grid label,
.grid .label {
  font-size: 13px;
}

.grid input[type='number'] {
  width: 120px;
}

.grid select {
  width: 155px;
}

.with-unit {
  display: flex;
  align-items: center;
  gap: 8px;
}

.radios {
  display: flex;
  align-items: center;
  gap: 12px;
}

.radios label {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.radios.off {
  color: var(--mutedColor);
}

.check {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  line-height: 1.35;
}

.check input {
  margin-top: 2px;
}

.check .hint {
  display: block;
}

.criteria-note {
  margin: 12px 0 0;
  color: var(--mutedColor);
  font-size: 11.5px;
  line-height: 1.4;
}

.window-row {
  display: grid;
  grid-template-columns: 110px 1fr;
  align-items: center;
  gap: 12px;
  margin-top: 10px;
  font-size: 13px;
}

.actions {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 8px;
  margin-top: 14px;
}

.actions .primary {
  padding: 5px 20px;
}

.validation {
  font-size: 12px;
  color: var(--warningColor);
}

.error-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border: 1px solid var(--dangerColor);
  border-radius: 6px;
  color: var(--dangerColor);
  font-size: 13px;
}

.error-banner button {
  margin-left: auto;
}

.statusbar {
  display: flex;
  align-items: center;
  gap: 16px;
  height: 26px;
  padding: 0 12px;
  flex-shrink: 0;
  background: var(--chromeBg);
  border-top: 1px solid var(--borderColor);
  font-size: 11.5px;
  color: var(--mutedColor);
  font-variant-numeric: tabular-nums;
}

.status-cell {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  border: 1px solid var(--mutedColor);
}

.dot.on {
  background: var(--accentColor);
  border-color: var(--accentColor);
  animation: pulse 1.2s ease-in-out infinite;
}

@keyframes pulse {
  50% {
    opacity: 0.35;
  }
}
</style>
