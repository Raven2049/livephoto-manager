<script setup lang="ts">
import { ref } from "vue";
import { useLibrary } from "../stores/library";

const lib = useLibrary();
const text = ref(lib.filter.text);
let timer: number | undefined;

function onText() {
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(() => {
    lib.filter.text = text.value;
    void lib.reload();
  }, 250);
}

const kinds: { label: string; value: number | null }[] = [
  { label: "全部", value: null },
  { label: "实况", value: 3 },
  { label: "照片", value: 1 },
  { label: "视频", value: 2 },
];

const integrities: { label: string; value: number }[] = [
  { label: "正常", value: 0 },
  { label: "不一致", value: 1 },
  { label: "残缺", value: 2 },
  { label: "仅静态", value: 3 },
  { label: "仅视频", value: 4 },
  { label: "疑似副本", value: 5 },
];

function setKind(v: number | null) {
  lib.filter.kind = v;
  void lib.reload();
}

function toggleIntegrity(v: number) {
  const set = new Set(lib.filter.integrity);
  if (set.has(v)) set.delete(v);
  else set.add(v);
  lib.filter.integrity = [...set];
  void lib.reload();
}

function onDate() {
  void lib.reload();
}

function clearAll() {
  text.value = "";
  lib.filter.text = "";
  lib.filter.kind = null;
  lib.filter.integrity = [];
  lib.filter.from = null;
  lib.filter.to = null;
  void lib.reload();
}
</script>

<template>
  <div class="filterbar">
    <input
      v-model="text"
      class="search"
      type="search"
      placeholder="搜索文件名"
      @input="onText"
    />

    <span class="group">
      <button
        v-for="k in kinds"
        :key="String(k.value)"
        :class="{ active: lib.filter.kind === k.value }"
        @click="setKind(k.value)"
      >
        {{ k.label }}
      </button>
    </span>

    <span class="group">
      <input type="date" v-model="lib.filter.from" @change="onDate" />
      <span class="muted">至</span>
      <input type="date" v-model="lib.filter.to" @change="onDate" />
    </span>

    <span class="group integ">
      <button
        v-for="i in integrities"
        :key="i.value"
        :class="{ active: lib.filter.integrity.includes(i.value) }"
        @click="toggleIntegrity(i.value)"
      >
        {{ i.label }}
      </button>
    </span>

    <span class="group">
      分段
      <select v-model="lib.granOverride">
        <option value="auto">自动</option>
        <option value="year">年</option>
        <option value="month">月</option>
        <option value="day">天</option>
      </select>
    </span>

    <button @click="clearAll">清空筛选</button>
  </div>
</template>

<style scoped>
.filterbar {
  padding: 6px 8px;
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
  border-top: 1px solid #222;
}
.search {
  min-width: 160px;
}
.group {
  display: inline-flex;
  gap: 4px;
  align-items: center;
}
.group button.active {
  background: #4af;
  color: #fff;
  border-color: #4af;
}
.integ button {
  font-size: 12px;
}
.muted {
  color: #888;
}
</style>
