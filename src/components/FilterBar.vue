<script setup lang="ts">
import { ref } from "vue";
import { useLibrary } from "../stores/library";
import AppIcon from "./AppIcon.vue";

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
  <div class="search">
    <AppIcon name="search" :size="15" />
    <input v-model="text" type="search" placeholder="搜索文件名" @input="onText" />
  </div>

  <div class="seg">
    <button
      v-for="k in kinds"
      :key="String(k.value)"
      :class="{ active: lib.filter.kind === k.value }"
      @click="setKind(k.value)"
    >
      {{ k.label }}
    </button>
  </div>

  <input
    class="field"
    type="date"
    v-model="lib.filter.from"
    @change="lib.reload"
  />
  <span class="muted" style="font-size: 12px">至</span>
  <input class="field" type="date" v-model="lib.filter.to" @change="lib.reload" />

  <button
    v-for="i in integrities"
    :key="i.value"
    class="chip"
    :class="{ active: lib.filter.integrity.includes(i.value) }"
    @click="toggleIntegrity(i.value)"
  >
    {{ i.label }}
  </button>

  <select class="field" v-model="lib.granOverride" title="时间线分段粒度">
    <option value="auto">分段：自动</option>
    <option value="year">年</option>
    <option value="month">月</option>
    <option value="day">天</option>
  </select>

  <button class="btn plain" title="清空筛选" @click="clearAll">清空</button>
</template>
