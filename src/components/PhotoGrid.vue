<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { lpmUrl } from "../lib/lpm";
import type { ImageItem } from "../stores/library";

const props = defineProps<{
  items: ImageItem[];
  columns: number;
  gap: number;
}>();

const scroller = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportH = ref(0);
const viewportW = ref(0);

// 瓦片边长由列数与容器宽算出（固定尺寸，便于虚拟化）
const tileW = computed(() => {
  const cols = Math.max(1, props.columns);
  return Math.floor((viewportW.value - props.gap * (cols + 1)) / cols);
});
const tileH = computed(() => tileW.value < 1 ? 1 : tileW.value);

const rows = computed(() => {
  const cols = Math.max(1, props.columns);
  return Math.ceil(props.items.length / cols);
});
const rowStride = computed(() => tileH.value + props.gap);
const totalH = computed(() => rows.value * rowStride.value + props.gap);

const startRow = computed(() =>
  Math.max(0, Math.floor(scrollTop.value / rowStride.value) - 1),
);
const endRow = computed(() => {
  const visible = Math.ceil(viewportH.value / rowStride.value) + 2;
  return Math.min(rows.value, startRow.value + visible);
});

interface Tile {
  item: ImageItem;
  index: number;
  x: number;
  y: number;
  size: number;
}

const visible = computed<Tile[]>(() => {
  const cols = Math.max(1, props.columns);
  const out: Tile[] = [];
  for (let r = startRow.value; r < endRow.value; r++) {
    for (let c = 0; c < cols; c++) {
      const index = r * cols + c;
      if (index >= props.items.length) break;
      out.push({
        item: props.items[index],
        index,
        x: props.gap + c * (tileW.value + props.gap),
        y: props.gap + r * rowStride.value,
        size: tileW.value,
      });
    }
  }
  return out;
});

function onScroll() {
  scrollTop.value = scroller.value?.scrollTop ?? 0;
}

/** 记录光标下的瓦片序号与相对行内的纵向偏移，供缩放后还原滚动位置。 */
function captureAnchor(clientX: number, clientY: number) {
  const el = scroller.value;
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  const localX = clientX - rect.left + el.scrollLeft;
  const localY = clientY - rect.top + el.scrollTop;
  const cols = Math.max(1, props.columns);
  const row = Math.floor(localY / rowStride.value);
  const col = Math.floor((localX - props.gap) / (tileW.value + props.gap));
  const index = row * cols + Math.max(0, col);
  return { index, fracY: localY - (props.gap + row * rowStride.value) };
}

function restoreAnchor(anchor: { index: number; fracY: number } | null) {
  const el = scroller.value;
  if (!el || !anchor) return;
  const cols = Math.max(1, props.columns);
  const row = Math.floor(anchor.index / cols);
  el.scrollTop = Math.max(0, props.gap + row * rowStride.value + anchor.fracY);
}

let ro: ResizeObserver | null = null;
onMounted(() => {
  const el = scroller.value;
  if (!el) return;
  ro = new ResizeObserver(() => {
    viewportH.value = el.clientHeight;
    viewportW.value = el.clientWidth;
  });
  ro.observe(el);
  viewportH.value = el.clientHeight;
  viewportW.value = el.clientWidth;
});
onBeforeUnmount(() => ro?.disconnect());

defineExpose({ el: scroller, captureAnchor, restoreAnchor });
</script>

<template>
  <div ref="scroller" class="scroller" @scroll.passive="onScroll">
    <div class="canvas" :style="{ height: totalH + 'px' }">
      <div
        v-for="t in visible"
        :key="t.index"
        class="tile"
        :style="{
          width: t.size + 'px',
          height: t.size + 'px',
          transform: `translate(${t.x}px, ${t.y}px)`,
        }"
      >
        <img :src="lpmUrl(t.item.path)" :alt="t.item.name" loading="lazy" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.scroller {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  overflow-x: hidden;
  background: #111;
}
.canvas {
  position: relative;
  width: 100%;
}
.tile {
  position: absolute;
  top: 0;
  left: 0;
  overflow: hidden;
  background: #1c1c1c;
  will-change: transform;
}
.tile img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
</style>
