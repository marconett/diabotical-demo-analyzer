<template>
  <div class="results">
    <div class="toolbar">
      <code class="header-line">{{ report.headerLine }}</code>
      <span class="spacer"></span>
      <span class="summary">
        {{ report.totalScenes }} scene{{ report.totalScenes === 1 ? '' : 's' }} across
        {{ report.demosWithPlayer }}/{{ report.demoCount }} demo{{
          report.demoCount === 1 ? '' : 's'
        }}
        <template v-if="report.cancelled"> · cancelled</template>
      </span>
      <button class="small" @click="copyText">{{ copied ? 'Copied' : 'Copy as text' }}</button>
      <button class="small" @click="showText = !showText">
        {{ showText ? 'Hide text' : 'Show text' }}
      </button>
    </div>

    <pre v-if="showText" class="text">{{ report.text }}</pre>

    <template v-else>
      <section v-for="d in withScenes" :key="d.path" class="demo">
        <div class="demo-head" :title="d.path">
          <strong>{{ d.fileName }}</strong>
          <span v-if="d.meta" class="meta">
            {{ d.meta.gameMode }} · {{ d.meta.mapName }} · {{ d.meta.appVersion }}
          </span>
          <span class="spacer"></span>
          <span class="count">{{ d.sceneCount }} scene{{ d.sceneCount === 1 ? '' : 's' }}</span>
          <span v-if="d.stoppedEarly" class="tag warn">stopped early</span>
        </div>
        <table>
          <thead>
            <tr>
              <th class="num">Time</th>
              <th>Player</th>
              <th class="num">Damage</th>
              <th class="num">Frags</th>
              <th class="num">Round</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(s, i) in d.scenes" :key="i" :title="s.line">
              <td class="num">
                <code>{{ s.clock }}</code>
              </td>
              <td>{{ s.player }}</td>
              <td class="num">{{ s.damage ?? '' }}</td>
              <td class="num">{{ s.frags ?? '' }}</td>
              <td class="num">
                <template v-if="s.round !== null"
                  >{{ s.round
                  }}<span v-if="s.damage === null" class="muted"> (winning frag)</span></template
                >
              </td>
            </tr>
          </tbody>
        </table>
        <div v-if="d.missingPlayers.length" class="note">
          No player named {{ d.missingPlayers.map(quote).join(', ') }} dealt damage or scored.
          <span v-if="d.playersSeen.length">Players seen: {{ d.playersSeen.join(', ') }}</span>
        </div>
      </section>

      <section v-if="withoutScenes.length" class="demo quiet">
        <button class="disclosure" @click="showQuiet = !showQuiet">
          {{ showQuiet ? '▾' : '▸' }} {{ withoutScenes.length }} demo{{
            withoutScenes.length === 1 ? '' : 's'
          }}
          without scenes
        </button>
        <ul v-if="showQuiet">
          <li v-for="d in withoutScenes" :key="d.path" :title="d.path">
            <strong>{{ d.fileName }}</strong>
            <span v-if="d.error" class="error"> {{ d.error }}</span>
            <span v-else-if="d.sceneCount === null" class="muted">
              no requested player active<template v-if="d.playersSeen.length">
                (seen: {{ d.playersSeen.join(', ') }})</template
              >
            </span>
            <span v-else class="muted"> no scene matched</span>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import type { RunReport } from '../lib/types';

const props = defineProps<{ report: RunReport }>();

const showText = ref(false);
const showQuiet = ref(false);
const copied = ref(false);

const withScenes = computed(() => props.report.demos.filter((d) => (d.sceneCount ?? 0) > 0));
const withoutScenes = computed(() => props.report.demos.filter((d) => (d.sceneCount ?? 0) === 0));

function quote(s: string) {
  return `'${s}'`;
}

async function copyText() {
  try {
    await navigator.clipboard.writeText(props.report.text);
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  } catch {
    showText.value = true;
  }
}
</script>

<style scoped>
.results {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.header-line {
  font-size: 12px;
  color: var(--mutedColor);
  user-select: text;
}

.summary {
  font-size: 12px;
  color: var(--mutedColor);
  white-space: nowrap;
}

.spacer {
  flex: 1;
}

.text {
  margin: 0;
  padding: 10px 12px;
  background: var(--chromeBg);
  border: 1px solid var(--borderColor);
  border-radius: 6px;
  white-space: pre;
  overflow: auto;
  user-select: text;
  cursor: text;
}

.demo {
  border: 1px solid var(--borderColor);
  border-radius: 6px;
  overflow: hidden;
}

.demo-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 5px 10px;
  background: var(--chromeBg);
  border-bottom: 1px solid var(--borderColor);
  font-size: 12.5px;
}

.meta,
.count,
.muted {
  color: var(--mutedColor);
}

.tag {
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: 0 5px;
  border: 1px solid var(--borderColor);
  border-radius: 3px;
  color: var(--mutedColor);
}

.tag.warn {
  color: var(--warningColor);
  border-color: var(--warningColor);
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

th,
td {
  padding: 3px 10px;
  text-align: left;
  border-bottom: 1px solid color-mix(in srgb, var(--borderColor) 60%, transparent);
}

th {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--mutedColor);
}

tbody tr:last-child td {
  border-bottom: none;
}

tbody tr:hover td {
  background: color-mix(in srgb, var(--shadeColor) 60%, transparent);
}

.num {
  text-align: right;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

td {
  user-select: text;
}

.note {
  padding: 5px 10px;
  font-size: 12px;
  color: var(--mutedColor);
  border-top: 1px solid var(--borderColor);
}

.quiet {
  padding: 4px 6px;
}

.disclosure {
  background: transparent;
  border: none;
  color: var(--mutedColor);
  padding: 2px 4px;
}

.quiet ul {
  margin: 4px 0 2px;
  padding: 0 0 0 20px;
  font-size: 12.5px;
}

.quiet li {
  padding: 1px 0;
}

.error {
  color: var(--dangerColor);
}
</style>
