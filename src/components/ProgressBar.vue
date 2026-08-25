<template>
  <div class="progress">
    <div class="bar">
      <div class="fill" :style="{ width: `${percent}%` }"></div>
    </div>
    <div class="caption">
      <span>{{ label }}</span>
      <span class="spacer"></span>
      <span>{{ progress.filesDone }}/{{ progress.filesTotal }} demos</span>
      <span v-if="progress.cacheHits">{{ progress.cacheHits }} cached</span>
      <span v-if="rate">{{ rate }}</span>
      <span>{{ fmtSeconds(progress.elapsedMs) }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { fmtRate, fmtSeconds } from '../lib/format';
import type { Progress } from '../lib/types';

const props = defineProps<{ progress: Progress }>();

const label = computed(() =>
  props.progress.phase === 'discover' ? 'Listing players' : 'Scanning demos',
);

const percent = computed(() => {
  const p = props.progress;
  if (p.finished) return 100;
  if (p.phase === 'run' && p.bytesTotal > 0)
    return Math.min(100, (p.bytesDone / p.bytesTotal) * 100);
  if (p.filesTotal > 0) return Math.min(100, (p.filesDone / p.filesTotal) * 100);
  return 0;
});

const rate = computed(() =>
  props.progress.phase === 'run' ? fmtRate(props.progress.bytesDone, props.progress.elapsedMs) : '',
);
</script>

<style scoped>
.progress {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.bar {
  height: 6px;
  background: var(--shadeColor);
  border: 1px solid var(--borderColor);
  border-radius: 3px;
  overflow: hidden;
}

.fill {
  height: 100%;
  background: var(--accentBg);
  transition: width 120ms linear;
}

.caption {
  display: flex;
  gap: 12px;
  font-size: 11.5px;
  color: var(--mutedColor);
  font-variant-numeric: tabular-nums;
}

.spacer {
  flex: 1;
}
</style>
