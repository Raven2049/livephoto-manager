<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useLibrary } from "../stores/library";
import AppIcon from "./AppIcon.vue";

const lib = useLibrary();
let timer: number | undefined;

function onInput(e: Event) {
  const value = (e.target as HTMLInputElement).value;
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(() => {
    lib.filter.text = value;
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

/* ---------- 「筛选」气泡：日期区间 + 完整性（收进此处以保持工具栏简洁） ---------- */
const open = ref(false);
const root = ref<HTMLElement | null>(null);

function onDocClick(e: MouseEvent) {
  if (open.value && root.value && !root.value.contains(e.target as Node)) open.value = false;
}
function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") open.value = false;
}
onMounted(() => {
  document.addEventListener("click", onDocClick);
  document.addEventListener("keydown", onKeydown);
});
onBeforeUnmount(() => {
  document.removeEventListener("click", onDocClick);
  document.removeEventListener("keydown", onKeydown);
});

/** 非默认的附加筛选数量（用于按钮角标）。 */
const activeCount = computed(
  () =>
    lib.filter.integrity.length + (lib.filter.from ? 1 : 0) + (lib.filter.to ? 1 : 0),
);
</script>

<template>
  <div class="search">
    <AppIcon name="search" :size="15" />
    <input
      :value="lib.filter.text"
      type="search"
      placeholder="搜索文件名"
      @input="onInput"
    />
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

  <div ref="root" class="filter-wrap">
    <button
      class="btn filter-btn"
      :class="{ active: activeCount > 0 }"
      :aria-expanded="open"
      @click.stop="open = !open"
    >
      <AppIcon name="filter" :size="15" />筛选
      <span v-if="activeCount" class="badge">{{ activeCount }}</span>
    </button>

    <div v-if="open" class="popover" @click.stop>
      <div class="pop-title">拍摄日期</div>
      <div class="pop-dates">
        <input class="field" type="date" v-model="lib.filter.from" @change="lib.reload" />
        <span class="muted" style="font-size: 12px">至</span>
        <input class="field" type="date" v-model="lib.filter.to" @change="lib.reload" />
      </div>

      <div class="pop-title">完整性</div>
      <div class="pop-chips">
        <button
          v-for="i in integrities"
          :key="i.value"
          class="chip"
          :class="{ active: lib.filter.integrity.includes(i.value) }"
          @click="toggleIntegrity(i.value)"
        >
          {{ i.label }}
        </button>
      </div>

      <button class="btn block" style="margin-top: 10px" @click="lib.clearFilters">
        清空筛选
      </button>
    </div>
  </div>
</template>

<style scoped>
.filter-wrap {
  position: relative;
}
.filter-btn {
  min-height: 36px;
  gap: 6px;
}
.filter-btn.active {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  color: var(--accent);
}
.badge {
  min-width: 17px;
  height: 17px;
  padding: 0 4px;
  border-radius: 9px;
  background: var(--accent);
  color: #fff;
  font-size: 11px;
  line-height: 17px;
  text-align: center;
}
.popover {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  z-index: 30;
  width: 300px;
  padding: 12px;
  background: var(--card);
  border: 1px solid var(--separator);
  border-radius: 14px;
  box-shadow: var(--shadow);
}
/* 小箭头指向触发按钮（HIG popovers：arrow points at the element that revealed it） */
.popover::before {
  content: "";
  position: absolute;
  top: -6px;
  right: 20px;
  width: 10px;
  height: 10px;
  background: var(--card);
  border-left: 1px solid var(--separator);
  border-top: 1px solid var(--separator);
  transform: rotate(45deg);
}
.pop-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--label-3);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 4px 2px 8px;
}
.pop-dates {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 14px;
}
.pop-dates .field {
  flex: 1;
  min-width: 0;
}
.pop-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
</style>
