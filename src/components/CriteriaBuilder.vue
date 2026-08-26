<template>
  <div class="builder">
    <div v-if="model.groups.length > 1" class="root-condition">
      Match
      <select v-model="model.condition" :disabled="disabled">
        <option value="AND">all groups</option>
        <option value="OR">any group</option>
      </select>
    </div>

    <section v-for="(group, groupIndex) in model.groups" :key="group.id" class="rule-group">
      <header>
        <strong>Group {{ groupIndex + 1 }}</strong>
        <span>match</span>
        <select v-model="group.condition" :disabled="disabled || group.rules.length < 2">
          <option value="AND">all criteria</option>
          <option value="OR">any criterion</option>
        </select>
        <button
          v-if="model.groups.length > 1"
          class="small remove"
          :disabled="disabled"
          title="Remove group"
          @click="removeGroup(groupIndex)"
        >
          Remove group
        </button>
      </header>

      <div v-for="(rule, ruleIndex) in group.rules" :key="rule.id" class="rule-row">
        <select
          :value="rule.kind"
          :disabled="disabled"
          aria-label="Criterion type"
          @change="changeKind(group, ruleIndex, eventKind($event))"
        >
          <option value="damage">Damage</option>
          <option value="frags">Frags</option>
          <option value="speed">Movement speed</option>
          <option value="accuracy">Weapon accuracy</option>
          <option value="siphonator">Siphonator active</option>
          <option value="flagCarrier">Carrying flag</option>
          <option value="roundWin">Round-winning frag</option>
          <option value="falloutDeath">Fallout death</option>
        </select>

        <template v-if="rule.kind === 'damage' || rule.kind === 'frags'">
          <span class="operator">≥</span>
          <input
            v-model.number="rule.minimum"
            type="number"
            min="0"
            step="1"
            :disabled="disabled"
          />
          <select v-model="rule.weapon" :disabled="disabled" aria-label="Weapon filter">
            <option :value="null">Any weapon</option>
            <option v-for="weapon in weapons" :key="weapon.id" :value="weapon.id">
              {{ weapon.name }}
            </option>
          </select>
        </template>

        <template v-else-if="rule.kind === 'speed'">
          <span class="operator">≥</span>
          <input
            v-model.number="rule.minimum"
            type="number"
            min="0"
            step="10"
            :disabled="disabled"
          />
          <span>units/s for</span>
          <input
            v-model.number="rule.duration"
            type="number"
            min="0"
            step="0.1"
            :disabled="disabled"
          />
          <span>seconds</span>
        </template>

        <template v-else-if="rule.kind === 'accuracy'">
          <span class="operator">≥</span>
          <input
            v-model.number="rule.minimum"
            type="number"
            min="0"
            max="100"
            step="0.1"
            :disabled="disabled"
          />
          <span>% with</span>
          <select v-model="rule.weapon" :disabled="disabled">
            <option v-for="weapon in weapons" :key="weapon.id" :value="weapon.id">
              {{ weapon.name }}
            </option>
          </select>
        </template>

        <template v-else-if="rule.kind === 'falloutDeath'">
          <select v-model="rule.exclude" :disabled="disabled" aria-label="Fallout behavior">
            <option :value="true">must not occur in window</option>
            <option :value="false">must occur in window</option>
          </select>
        </template>

        <span v-else class="state-note">at any time in the window</span>

        <button
          class="icon-button"
          :disabled="disabled || (model.groups.length === 1 && group.rules.length === 1)"
          title="Remove criterion"
          aria-label="Remove criterion"
          @click="removeRule(groupIndex, ruleIndex)"
        >
          ×
        </button>
      </div>

      <button class="small add" :disabled="disabled" @click="addRule(groupIndex)">
        + Add criterion
      </button>
    </section>

    <button class="small add-group" :disabled="disabled" @click="addGroup">+ Add group</button>
  </div>
</template>

<script setup lang="ts">
import type { RuleGroup, RuleKind, RuleQuery, SearchRule } from '../lib/types';
import { weapons } from '../lib/weapons';

defineProps<{ disabled?: boolean }>();
const model = defineModel<RuleQuery>({ required: true });

let nextId = Date.now();
const id = (prefix: string) => `${prefix}-${nextId++}`;

function makeRule(kind: RuleKind): SearchRule {
  const ruleId = id('rule');
  switch (kind) {
    case 'damage':
      return { id: ruleId, kind, minimum: 500, weapon: null };
    case 'frags':
      return { id: ruleId, kind, minimum: 4, weapon: null };
    case 'speed':
      return { id: ruleId, kind, minimum: 900, duration: 0.5 };
    case 'accuracy':
      return { id: ruleId, kind, minimum: 40, weapon: 5 };
    case 'falloutDeath':
      return { id: ruleId, kind, exclude: true };
    default:
      return { id: ruleId, kind };
  }
}

function eventKind(event: Event): RuleKind {
  return (event.target as HTMLSelectElement).value as RuleKind;
}

function changeKind(group: RuleGroup, index: number, kind: RuleKind) {
  const replacement = makeRule(kind);
  replacement.id = group.rules[index].id;
  group.rules[index] = replacement;
}

function addRule(groupIndex: number) {
  model.value.groups[groupIndex].rules.push(makeRule('damage'));
}

function removeRule(groupIndex: number, ruleIndex: number) {
  const group = model.value.groups[groupIndex];
  group.rules.splice(ruleIndex, 1);
  if (group.rules.length === 0 && model.value.groups.length > 1) {
    model.value.groups.splice(groupIndex, 1);
  }
}

function addGroup() {
  model.value.groups.push({ id: id('group'), condition: 'AND', rules: [makeRule('damage')] });
}

function removeGroup(index: number) {
  model.value.groups.splice(index, 1);
}
</script>

<style scoped>
.builder {
  display: flex;
  flex-direction: column;
  gap: 9px;
}

.root-condition {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 13px;
}

.rule-group {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 9px;
  border: 1px solid var(--borderColor);
  border-radius: 6px;
  background: color-mix(in srgb, var(--shadeColor) 35%, transparent);
}

.rule-group header {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
}

.rule-group header .remove {
  margin-left: auto;
}

.rule-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 12px;
}

.rule-row > select:first-child {
  width: 126px;
  flex-shrink: 0;
}

.rule-row input[type='number'] {
  width: 65px;
}

.rule-row select[aria-label='Weapon filter'],
.rule-row span + select {
  min-width: 105px;
  flex: 1;
}

.operator,
.state-note {
  color: var(--mutedColor);
}

.state-note {
  flex: 1;
}

.icon-button {
  width: 23px;
  height: 23px;
  padding: 0;
  margin-left: auto;
  font-size: 17px;
  line-height: 1;
}

.add {
  align-self: flex-start;
}

.add-group {
  align-self: flex-start;
}
</style>
