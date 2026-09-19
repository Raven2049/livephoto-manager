<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { lpmUrl } from "../lib/lpm";
import type { AssetRow } from "../stores/library";

const props = defineProps<{
  assets: AssetRow[];
  index: number;
}>();
const emit = defineEmits<{ close: []; navigate: [index: number] }>();

const src = ref<string | null>(null);
const loading = ref(false);
const failed = ref(false);
let token = 0;
/** 已生成的查看图 URL（内容哈希命名，同一内容复用）。 */
const cache = new Map<number, string>();

const asset = computed(() => props.assets[props.index] ?? null);

/* ---------- 缩放 / 平移（以鼠标落点为锚点） ---------- */
const stageEl = ref<HTMLElement | null>(null);
const scale = ref(1);
const tx = ref(0);
const ty = ref(0);
const MAX_SCALE = 8;
let dragging = false;
let moved = false;
let lastX = 0;
let lastY = 0;

function resetView() {
  scale.value = 1;
  tx.value = 0;
  ty.value = 0;
}

/** 以视口坐标 (cx,cy) 为不动点缩放。 */
function zoomAt(cx: number, cy: number, factor: number) {
  const el = stageEl.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  const bx = r.left + r.width / 2;
  const by = r.top + r.height / 2;
  const s = scale.value;
  const next = Math.min(MAX_SCALE, Math.max(1, s * factor));
  // 光标下的图像点保持不变：d = (C - B - t) / s，t' = C - B - s' * d
  const dx = (cx - bx - tx.value) / s;
  const dy = (cy - by - ty.value) / s;
  tx.value = cx - bx - next * dx;
  ty.value = cy - by - next * dy;
  scale.value = next;
  if (next === 1) {
    tx.value = 0;
    ty.value = 0;
  }
}

function onWheel(e: WheelEvent) {
  e.preventDefault();
  zoomAt(e.clientX, e.clientY, e.deltaY < 0 ? 1.15 : 1 / 1.15);
}
function onPointerDown(e: PointerEvent) {
  moved = false;
  if (scale.value <= 1) return;
  dragging = true;
  lastX = e.clientX;
  lastY = e.clientY;
  (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
}
function onPointerMove(e: PointerEvent) {
  if (!dragging) return;
  if (Math.abs(e.clientX - lastX) + Math.abs(e.clientY - lastY) > 2) moved = true;
  tx.value += e.clientX - lastX;
  ty.value += e.clientY - lastY;
  lastX = e.clientX;
  lastY = e.clientY;
}
function onPointerUp() {
  dragging = false;
}
/** 点击空白关闭（刚拖动平移过的那一次释放不算点击，避免误关）。 */
function onStageClick() {
  if (moved) return;
  emit("close");
}

async function load(i: number) {
  const a = props.assets[i];
  if (!a) return;
  const hit = cache.get(a.id);
  if (hit) {
    src.value = hit;
    loading.value = false;
    failed.value = false;
    return;
  }
  const my = ++token;
  loading.value = true;
  failed.value = false;
  src.value = null;
  try {
    const p = await invoke<string>("ensure_view", { assetId: a.id });
    const url = lpmUrl(p);
    cache.set(a.id, url);
    if (my !== token) return;
    src.value = url;
  } catch {
    if (my === token) failed.value = true;
  } finally {
    if (my === token) loading.value = false;
  }
}

/** 预取相邻张，翻页更快。 */
function prefetch(i: number) {
  const a = props.assets[i];
  if (!a || cache.has(a.id)) return;
  void invoke<string>("ensure_view", { assetId: a.id })
    .then((p) => cache.set(a.id, lpmUrl(p)))
    .catch(() => {});
}

function prev() {
  if (props.index > 0) emit("navigate", props.index - 1);
}
function next() {
  if (props.index < props.assets.length - 1) emit("navigate", props.index + 1);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
  else if (e.key === "ArrowLeft") prev();
  else if (e.key === "ArrowRight") next();
}

watch(
  () => props.index,
  (i) => {
    resetView();
    void load(i);
    prefetch(i + 1);
    prefetch(i - 1);
  },
  { immediate: true },
);

onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="viewer" @click.self="emit('close')">
    <button class="v-close" aria-label="关闭" @click="emit('close')">✕</button>

    <button class="v-nav prev" :disabled="index <= 0" aria-label="上一张" @click.stop="prev">
      ‹
    </button>

    <div
      ref="stageEl"
      class="v-stage"
      :class="{ zoomed: scale > 1 }"
      @click.self="onStageClick"
      @wheel="onWheel"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
    >
      <div v-if="loading" class="v-msg">正在生成大图…</div>
      <div v-else-if="failed" class="v-msg">无法显示这张图</div>
      <img
        v-else-if="src"
        :src="src"
        :alt="asset?.base_name"
        :style="{ transform: `translate(${tx}px, ${ty}px) scale(${scale})` }"
        draggable="false"
      />
    </div>

    <button
      class="v-nav next"
      :disabled="index >= assets.length - 1"
      aria-label="下一张"
      @click.stop="next"
    >
      ›
    </button>

    <div class="v-caption">
      <span>{{ asset?.base_name }}</span>
      <span class="muted">{{ index + 1 }} / {{ assets.length }}</span>
    </div>
  </div>
</template>

<style scoped>
.viewer {
  position: fixed;
  inset: 0;
  z-index: 60;
  background: rgba(0, 0, 0, 0.92);
  display: grid;
  place-items: center;
}
.v-stage {
  position: absolute;
  inset: 0;
  /* flex + 确定高度，max-height:100% 才会生效（grid 的 auto 行不会约束） */
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 56px 72px 64px;
  box-sizing: border-box;
  overflow: hidden;
  cursor: default;
  touch-action: none;
}
.v-stage.zoomed {
  cursor: grab;
}
.v-stage.zoomed:active {
  cursor: grabbing;
}
.v-stage img {
  max-width: 100%;
  max-height: 100%;
  width: auto;
  height: auto;
  object-fit: contain;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.6);
  transform-origin: center center;
  user-select: none;
  -webkit-user-drag: none;
}
.v-msg {
  color: rgba(255, 255, 255, 0.75);
  font-size: 14px;
}
.v-close {
  position: absolute;
  top: 14px;
  right: 16px;
  width: 36px;
  height: 36px;
  border: 0;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.14);
  color: #fff;
  font-size: 16px;
  z-index: 2;
}
.v-close:hover {
  background: rgba(255, 255, 255, 0.26);
}
.v-nav {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  width: 48px;
  height: 72px;
  border: 0;
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
  font-size: 34px;
  line-height: 1;
  z-index: 2;
  border-radius: 10px;
}
.v-nav:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.22);
}
.v-nav:disabled {
  opacity: 0.2;
  cursor: default;
}
.v-nav.prev {
  left: 16px;
}
.v-nav.next {
  right: 16px;
}
.v-caption {
  position: absolute;
  bottom: 16px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  gap: 10px;
  padding: 6px 14px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.12);
  color: #fff;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  z-index: 2;
}
.v-caption .muted {
  color: rgba(255, 255, 255, 0.6);
}
</style>
