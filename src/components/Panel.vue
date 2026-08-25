<template>
  <section class="panel">
    <h3 v-if="title" class="panel-title">{{ title }}</h3>
    <div class="panel-body">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
defineProps<{ title?: string }>();
</script>

<style scoped>
.panel {
  border: 1px solid var(--borderColor);
  border-radius: 6px;
  background: var(--bgColor);
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.panel-title {
  margin: 0;
  padding: 5px 12px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--mutedColor);
  background: var(--chromeBg);
  border-bottom: 1px solid var(--borderColor);
  border-radius: 5px 5px 0 0;
}

.panel-body {
  padding: 12px;
  min-width: 0;
}
</style>
