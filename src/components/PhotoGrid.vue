<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { lpmUrl } from "../lib/lpm";
import { buildLayout, headerHeight, type TileCell } from "../lib/gridLayout";
import type { AssetGroup, Granularity } from "../lib/timeline";
import type { AssetRow } from "../stores/library";

const props = defineProps<{
  groups: AssetGroup[];
  columns: number;
  gap: number;
  granularity: Granularity;
  selected: Set<number>;
  hasMore: boolean;
}>();

const emit = defineEmits<{
  /** 悬停进入某个瓦片；`asset` 可能为 null（下标越界时）。 */
  hover: [payload: { asset: AssetRow | null; rect: DOMRect }];
  "hover-out": [];
  /** 滚动中：父组件据此抑制悬停预览（设计 §7.3）。 */
  suppress: [];
  "toggle-select": [id: number];
  /** 滚动接近底部，父组件据此加载下一页。 */
  "near-end": [];
}>();

function onTileEnter(cell: TileCell, e: MouseEvent) {
  const el = e.currentTarget as HTMLElement | null;
  if (!el) return;
  emit("hover", { asset: cell.asset, rect: el.getBoundingClientRect() });
}

function pathOf(a: AssetRow): string {
  return (a.thumb_path || a.still_path) as string;
}

// 1x1 透明 GIF：未加载的瓦片用它占位，避免滚动途中批量发起请求。
const PLACEHOLDER =
  "data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==";

const scroller = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportH = ref(0);
const viewportW = ref(0);

// 滚动停稳后才给新瓦片挂 src；已加载过的 index 永久保留，不回退成占位图。
const settled = ref(false);
const loaded = new Set<number>();
let settleTimer: number | undefined;

// 瓦片边长由列数与容器宽算出（固定尺寸，便于虚拟化）
const tileW = computed(() => {
  const cols = Math.max(1, props.columns);
  return Math.floor((viewportW.value - props.gap * (cols + 1)) / cols);
});
const tileStride = computed(() => (tileW.value < 1 ? 1 : tileW.value) + props.gap);

const hh = computed(() => headerHeight(props.granularity));
const layout = computed(() =>
  buildLayout(props.groups, props.columns, tileW.value, props.gap, props.hasMore, hh.value),
);

// 行数约千级，每帧线性过滤开销可忽略。
const visibleRows = computed(() => {
  const top = scrollTop.value;
  const bottom = top + viewportH.value;
  return layout.value.rows.filter((r) => r.y + r.h >= top - 2 && r.y <= bottom + 2);
});

function markVisibleLoaded() {
  for (const row of visibleRows.value) {
    if (row.type === "tiles") for (const c of row.cells) loaded.add(c.index);
  }
}

function tileSrc(index: number, path: string): string {
  return settled.value || loaded.has(index) ? lpmUrl(path) : PLACEHOLDER;
}

/** 接近底部时请求下一页（父组件的 loading 守卫去重）。 */
function maybeLoadMore() {
  if (!props.hasMore) return;
  const total = layout.value.totalH;
  if (scrollTop.value + viewportH.value >= total - tileStride.value * 3) {
    emit("near-end");
  }
}

let ticking = false;
function onScroll() {
  // 滚动期间与停稳后的短时间内都不触发悬停预览（设计 §7.3）。
  emit("suppress");

  // 快速拖动滚动条时一帧可能触发多次 scroll，用 rAF 节流到一帧一次重渲染。
  if (ticking) return;
  ticking = true;
  requestAnimationFrame(() => {
    scrollTop.value = scroller.value?.scrollTop ?? 0;
    ticking = false;
    maybeLoadMore();
  });

  // 滚动途中只更新布局，不给新瓦片加载图片。
  settled.value = false;
  if (settleTimer !== undefined) clearTimeout(settleTimer);
  settleTimer = window.setTimeout(() => {
    markVisibleLoaded();
    settled.value = true;
  }, 150);
}

interface Anchor {
  index: number;
  /** 光标在瓦片行内的纵向比例。 */
  fy: number;
  /** 光标相对视口顶部的 y。 */
  viewportY: number;
}

/** 记录光标下的瓦片序号及其纵向比例，供缩放后还原滚动位置。 */
function captureAnchor(clientX: number, clientY: number): Anchor | null {
  const el = scroller.value;
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  const localY = clientY - rect.top + el.scrollTop;
  const stride = tileStride.value;

  for (const row of layout.value.rows) {
    if (row.type !== "tiles") {
      if (localY < row.y) {
        // 落在日期头/页脚里，锚到下一瓦片行第一格。
        const next = layout.value.rows.find((r) => r.type === "tiles" && r.y >= row.y);
        if (next && next.type === "tiles" && next.cells[0]) {
          return { index: next.cells[0].index, fy: 0, viewportY: clientY - rect.top };
        }
      }
      continue;
    }
    if (localY >= row.y && localY < row.y + row.h) {
      const localX = clientX - rect.left + el.scrollLeft;
      const col = Math.max(
        0,
        Math.min(row.cells.length - 1, Math.floor((localX - props.gap) / stride)),
      );
      const cell = row.cells[col] ?? row.cells[row.cells.length - 1];
      if (!cell) return null;
      return {
        index: cell.index,
        fy: (localY - row.y) / row.h,
        viewportY: clientY - rect.top,
      };
    }
  }
  return null;
}

function restoreAnchor(anchor: Anchor | null) {
  const el = scroller.value;
  if (!el || !anchor) return;
  const y = layout.value.indexToY(anchor.index);
  el.scrollTop = Math.max(0, y + anchor.fy * tileStride.value - anchor.viewportY);
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

  // 首屏：等一帧拿到尺寸后，加载当前视口。
  nextTick(() => {
    markVisibleLoaded();
    settled.value = true;
    maybeLoadMore();
  });
});
onBeforeUnmount(() => {
  ro?.disconnect();
  if (settleTimer !== undefined) clearTimeout(settleTimer);
});

// 追加分页后高度增长、或视口变化时，检查是否需要继续加载（填满视口）。
watch([() => layout.value.totalH, viewportH], () => maybeLoadMore());

defineExpose({ el: scroller, captureAnchor, restoreAnchor });
</script>

<template>
  <div ref="scroller" class="scroller" @scroll.passive="onScroll">
    <div class="canvas" :style="{ height: layout.totalH + 'px' }">
      <template v-for="row in visibleRows" :key="row.key">
        <div
          v-if="row.type === 'header'"
          class="date-header"
          :style="{ transform: `translateY(${row.y}px)`, height: row.h + 'px' }"
        >
          {{ row.label }}<span>· {{ row.count }} 张</span>
        </div>
        <template v-else-if="row.type === 'tiles'">
          <div
            v-for="cell in row.cells"
            :key="cell.index"
            class="tile"
            :class="{ selected: props.selected.has(cell.asset.id) }"
            :style="{
              width: cell.size + 'px',
              height: cell.size + 'px',
              transform: `translate(${cell.x}px, ${row.y}px)`,
            }"
            @mouseenter="onTileEnter(cell, $event)"
            @mouseleave="emit('hover-out')"
            @click="emit('toggle-select', cell.asset.id)"
          >
            <img
              :src="tileSrc(cell.index, pathOf(cell.asset))"
              :alt="cell.asset.base_name"
              loading="lazy"
              decoding="async"
            />
            <span v-if="props.selected.has(cell.asset.id)" class="check">✓</span>
          </div>
        </template>
        <div
          v-else
          class="load-footer"
          :style="{ transform: `translateY(${row.y}px)` }"
        >
          加载中…
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.scroller {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--bg);
}
.canvas {
  position: relative;
  width: 100%;
}
.date-header {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 0 16px;
  box-sizing: border-box;
  color: var(--label);
  font-size: 17px;
  font-weight: 650;
  letter-spacing: -0.01em;
  background: linear-gradient(180deg, var(--bg) 72%, transparent);
  z-index: 1;
}
.date-header span {
  font-size: 13px;
  font-weight: 400;
  color: var(--label-2);
}
.tile {
  position: absolute;
  top: 0;
  left: 0;
  overflow: hidden;
  border-radius: var(--r-tile);
  background: var(--tile);
  cursor: pointer;
  transition: transform 0.16s cubic-bezier(0.2, 0.8, 0.2, 1), box-shadow 0.16s;
}
.tile:hover {
  transform: scale(1.03);
  box-shadow: var(--shadow);
  z-index: 2;
}
.tile img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.tile.selected {
  outline: 3px solid var(--accent);
  outline-offset: -3px;
}
.tile.selected:hover {
  transform: none;
}
.check {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--accent);
  color: #fff;
  font-size: 14px;
  line-height: 24px;
  text-align: center;
  border: 2px solid #fff;
  box-sizing: border-box;
}
.load-footer {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--label-2);
  font-size: 13px;
}
</style>
