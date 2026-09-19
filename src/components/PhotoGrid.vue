<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { lpmUrl } from "../lib/lpm";
import { buildLayout, headerHeight, type TileCell } from "../lib/gridLayout";
import type { AssetGroup, Granularity } from "../lib/timeline";
import type { AssetRow } from "../stores/library";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  groups: AssetGroup[];
  columns: number;
  gap: number;
  granularity: Granularity;
  selected: Set<number>;
  hasMore: boolean;
  /** 缩放到最大档时使用单列大图流。 */
  masonry: boolean;
  /** 单列流里已就绪的高清大图路径（asset id → 路径）。 */
  largeById: Record<number, string>;
  /** 是否正在加载下一页（用于页脚文案）。 */
  loading: boolean;
}>();

const emit = defineEmits<{
  /** 悬停进入某个瓦片；`asset` 可能为 null（下标越界时）。 */
  hover: [payload: { asset: AssetRow | null; rect: DOMRect }];
  "hover-out": [];
  /** 滚动中：父组件据此抑制悬停预览（设计 §7.3）。 */
  suppress: [];
  /** 长按完成 / 选择模式下点按：切换该条目选中态。 */
  "toggle-select": [id: number];
  /** 拖拽涂抹：把某条目设为选中(true)/未选(false)。 */
  "set-selected": [id: number, value: boolean];
  /** 单列流：请求这些条目的高清大图（父组件按需生成并缓存）。 */
  "need-large": [ids: number[]];
  /** 滚动接近底部，父组件据此加载下一页。 */
  "near-end": [];
  /** 瓦片右键：请求在 (x,y) 处打开上下文菜单。 */
  "context-menu": [id: number, x: number, y: number];
  /** 键盘 Delete：请求删除当前选中项。 */
  "request-delete": [];
  /** 打开单张查看（单击未选中项 / 键盘 Enter）。 */
  open: [id: number];
  /** 当前所在分段：`pinned` 为该段标题已滚出顶部（父组件据此显示固定分段条）。 */
  section: [payload: { label: string; pinned: boolean }];
}>();

function onTileEnter(cell: TileCell, e: MouseEvent) {
  const el = e.currentTarget as HTMLElement | null;
  if (!el) return;
  emit("hover", { asset: cell.asset, rect: el.getBoundingClientRect() });
}

function pathOf(a: AssetRow): string {
  // 单列流若已生成高清大图则优先用它，否则退回缩略图/原图。
  return props.largeById[a.id] || ((a.thumb_path || a.still_path) as string);
}

/** 通知父组件为当前可见条目按需生成高清大图（仅单列流）。 */
function emitNeedLarge() {
  if (!props.masonry) return;
  const ids: number[] = [];
  for (const row of visibleRows.value) {
    if (row.type === "tiles") for (const c of row.cells) ids.push(c.asset.id);
  }
  if (ids.length) emit("need-large", ids);
}

/* ---------- 长按选中 + 拖拽涂抹选择 ----------
 * - 按住不动满 LONG_PRESS_MS：右上角计时环填满 → 选中该张（不触发播放）
 * - 按住后移动超过 DRAG_START_DIST：直接进入拖拽多选（无需先长按）
 * - 未选中的图：单纯点击不选中（防误触）
 * - 已选中的图：单纯单击即取消选择 */
const LONG_PRESS_MS = 500;
const DRAG_START_DIST = 10;

const armingId = ref<number | null>(null);
let pressTimer: number | undefined;
let pressedId: number | null = null;
let pressedWasSelected = false;
let didPaint = false;
let didLongPress = false;
let pressStart = { x: 0, y: 0 };
let painting = false;
let paintValue = true;
let lastPaintId: number | null = null;
let listening = false;
/** 本次拖拽中已涂过的条目 → 拖拽开始前的选中态（回拖时还原）。 */
const paintedOriginal = new Map<number, boolean>();
/** 本次拖拽经过的条目顺序，用于识别折返。 */
let path: number[] = [];

function clearPressTimer() {
  if (pressTimer !== undefined) {
    clearTimeout(pressTimer);
    pressTimer = undefined;
  }
  armingId.value = null;
}

function tileIdAt(x: number, y: number): number | null {
  const el = document.elementFromPoint(x, y);
  const tile = el?.closest(".tile") as HTMLElement | null;
  const id = tile?.dataset.id;
  return id ? Number(id) : null;
}

function startListening() {
  if (listening) return;
  listening = true;
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", onPointerUp);
  window.addEventListener("pointercancel", onPointerUp);
}
function stopListening() {
  listening = false;
  window.removeEventListener("pointermove", onPointerMove);
  window.removeEventListener("pointerup", onPointerUp);
  window.removeEventListener("pointercancel", onPointerUp);
}

function onTilePointerDown(cell: TileCell, e: PointerEvent) {
  if (e.button !== 0) return;
  // 立刻收起/抑制悬停预览：长按与拖拽期间都不该播放，也不能遮住右上角图标
  emit("suppress");
  pressStart = { x: e.clientX, y: e.clientY };
  pressedId = cell.asset.id;
  pressedWasSelected = props.selected.has(cell.asset.id);
  didPaint = false;
  didLongPress = false;
  painting = false;
  lastPaintId = null;
  armingId.value = pressedId;
  pressTimer = window.setTimeout(finishLongPress, LONG_PRESS_MS);
  startListening();
}

function finishLongPress() {
  pressTimer = undefined;
  armingId.value = null;
  if (pressedId === null) return;
  didLongPress = true;
  const id = pressedId;
  lastPaintId = id;
  emit("toggle-select", id);
}

/** 正向涂抹一张：记录拖动前状态并按 paintValue 选中/取消。 */
function paintForward(id: number) {
  paintedOriginal.set(id, props.selected.has(id));
  emit("set-selected", id, paintValue);
  path.push(id);
}

/** 逆向撤回一张：还原到拖动前状态，并移出记录（再正向经过可重新涂上）。 */
function paintUndo(id: number) {
  const original = paintedOriginal.get(id);
  if (original !== undefined) {
    emit("set-selected", id, original);
    paintedOriginal.delete(id);
  }
}

/** 从「按下」转为「拖拽涂抹」：以按下那张的当前状态决定是选中还是取消。 */
function beginPaint() {
  clearPressTimer();
  painting = true;
  didPaint = true;
  path = [];
  const pressed = pressedId;
  paintValue = !(pressed !== null && props.selected.has(pressed));
  if (pressed !== null) {
    lastPaintId = pressed;
    paintForward(pressed);
  }
}

function onPointerMove(e: PointerEvent) {
  if (pressTimer !== undefined) {
    if (Math.hypot(e.clientX - pressStart.x, e.clientY - pressStart.y) > DRAG_START_DIST) {
      beginPaint();
    } else {
      return;
    }
  }
  if (!painting) return;
  const id = tileIdAt(e.clientX, e.clientY);
  if (id === null || id === lastPaintId) return;
  // 折返：回到路径倒数第二张 → 先撤回刚离开的那张（折返点）
  if (path.length >= 2 && id === path[path.length - 2]) {
    paintUndo(path[path.length - 1]);
    path.pop();
    // 一路退回起点：连起点也一起撤销，整笔清空
    if (path.length === 1) {
      paintUndo(path[0]);
      path.pop();
    }
    lastPaintId = id;
    return;
  }
  // 再次进入已涂过的（路径交叉）：还原
  if (paintedOriginal.has(id)) {
    paintUndo(id);
    const idx = path.lastIndexOf(id);
    if (idx >= 0) path.splice(idx, 1);
    lastPaintId = id;
    return;
  }
  // 新进入：若刚从起点清空，先把起点重新纳入
  if (path.length === 0 && pressedId !== null && !paintedOriginal.has(pressedId)) {
    paintForward(pressedId);
  }
  lastPaintId = id;
  paintForward(id);
}

function onPointerUp(e: PointerEvent) {
  // 单纯单击（未拖拽/未长按）：
  //   已选中 → 取消选择；未选中 → 打开单张查看。
  if (e.type === "pointerup" && !didPaint && !didLongPress && pressedId !== null) {
    if (pressedWasSelected) emit("toggle-select", pressedId);
    else emit("open", pressedId);
  }
  clearPressTimer();
  pressedId = null;
  pressedWasSelected = false;
  painting = false;
  lastPaintId = null;
  paintedOriginal.clear();
  path = [];
  stopListening();
}

/* ---------- 键盘可访问：方向键移动焦点，Space 选择，Delete 删除 ---------- */
const focusedIndex = ref(0);
const itemCount = computed(() => props.groups.reduce((n, g) => n + g.assets.length, 0));
// 始终让「首个可见瓦片」可 Tab 进入；用户一旦键盘移动过，就跟随用户位置。
const tabbableIndex = computed(() => {
  const fv = firstVisibleIndex();
  return focusedIndex.value >= fv && focusedIndex.value < fv + 200 ? focusedIndex.value : fv;
});

/** 只让目标瓦片滚入视口（不主动 focus，避免浏览器/点击引发的意外滚动）。 */
function scrollIndexIntoView(index: number) {
  const el = scroller.value;
  if (!el) return;
  const top = layout.value.indexToY(index);
  const h = layout.value.indexHeight(index);
  const bottom = top + h;
  if (top < el.scrollTop + 40) el.scrollTop = Math.max(0, top - 40);
  else if (bottom > el.scrollTop + el.clientHeight - 40) {
    el.scrollTop = bottom - el.clientHeight + 40;
  }
}

/** 键盘导航：把目标瓦片滚入视口并聚焦。 */
function focusIndex(index: number) {
  scrollIndexIntoView(index);
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      scroller.value?.querySelector<HTMLElement>(`.tile[data-index="${index}"]`)?.focus();
    }),
  );
}

function onTileKeydown(cell: TileCell, e: KeyboardEvent) {
  const cols = effectiveColumns.value;
  let target = cell.index;
  switch (e.key) {
    case "ArrowRight":
      target += 1;
      break;
    case "ArrowLeft":
      target -= 1;
      break;
    case "ArrowDown":
      target += cols;
      break;
    case "ArrowUp":
      target -= cols;
      break;
    case " ":
    case "Spacebar":
      e.preventDefault();
      emit("toggle-select", cell.asset.id);
      return;
    case "Delete":
    case "Backspace":
      e.preventDefault();
      emit("request-delete");
      return;
    case "Enter":
      e.preventDefault();
      emit("open", cell.asset.id);
      return;
    default:
      return;
  }
  e.preventDefault();
  if (target < 0 || target >= itemCount.value) return;
  focusedIndex.value = target;
  focusIndex(target);
}

function onTileContextMenu(cell: TileCell, e: MouseEvent) {
  e.preventDefault();
  emit("context-menu", cell.asset.id, e.clientX, e.clientY);
}

/** 首个可见瓦片的序号（用作漫游 tabindex 的落点，避免 Tab 跳到列表顶部）。 */
function firstVisibleIndex(): number {
  const row = visibleRows.value.find((r) => r.type === "tiles");
  return row && row.type === "tiles" && row.cells[0] ? row.cells[0].index : 0;
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

// 瀑布流（最大放大档）= 单列等宽大图；其余档位用等边网格。
const effectiveColumns = computed(() => (props.masonry ? 1 : Math.max(1, props.columns)));
// 瓦片边长由列数与容器宽算出（固定尺寸，便于虚拟化）
const tileW = computed(() => {
  const cols = effectiveColumns.value;
  return Math.floor((viewportW.value - props.gap * (cols + 1)) / cols);
});
const tileStride = computed(() => (tileW.value < 1 ? 1 : tileW.value) + props.gap);

const hh = computed(() => headerHeight(props.granularity));
const layout = computed(() =>
  buildLayout(
    props.groups,
    effectiveColumns.value,
    tileW.value,
    props.gap,
    props.hasMore,
    hh.value,
    props.masonry,
  ),
);

// 视口外预载范围：按「行」计，密集网格也只多渲染 2 行（一屏会多渲染几百个瓦片）。
const preloadPx = computed(() =>
  props.masonry ? viewportH.value * 0.8 : tileStride.value * 2,
);

// 渲染范围 = 视口 + 预载带（约千级行，线性过滤开销可忽略）。
const visibleRows = computed(() => {
  const top = scrollTop.value;
  const bottom = top + viewportH.value;
  const pre = preloadPx.value;
  return layout.value.rows.filter((r) => r.y + r.h >= top - pre - 2 && r.y <= bottom + pre + 2);
});

function markVisibleLoaded() {
  for (const row of visibleRows.value) {
    if (row.type === "tiles") for (const c of row.cells) loaded.add(c.index);
  }
}

function tileSrc(index: number, path: string): string {
  return settled.value || loaded.has(index) ? lpmUrl(path) : PLACEHOLDER;
}

/* ---------- 滚动停稳后，鼠标停留的那张也自动进入悬停预览 ----------
 * 滚动时瓦片被替换/新增，光标没动就不会有 mouseenter，于是不会播放。
 * 这里记录鼠标位置，滚动停稳并过了抑制窗口后主动补一次 hover。 */
let lastMouse = { x: 0, y: 0, inside: false };
let hoverResumeTimer: number | undefined;

function onMouseMove(e: MouseEvent) {
  lastMouse = { x: e.clientX, y: e.clientY, inside: true };
}
function onMouseLeave() {
  lastMouse.inside = false;
}

function findCellByIndex(index: number): TileCell | null {
  for (const row of visibleRows.value) {
    if (row.type !== "tiles") continue;
    for (const c of row.cells) if (c.index === index) return c;
  }
  return null;
}

function resumeHoverUnderPointer() {
  if (!lastMouse.inside) return;
  const el = document.elementFromPoint(lastMouse.x, lastMouse.y);
  const tile = el?.closest(".tile") as HTMLElement | null;
  if (!tile) return;
  const index = Number(tile.dataset.index);
  if (Number.isNaN(index)) return;
  const cell = findCellByIndex(index);
  if (!cell) return;
  emit("hover", { asset: cell.asset, rect: tile.getBoundingClientRect() });
}

/* ---------- 当前分段（供顶部固定分段条） ---------- */
let lastSectionKey = "";
function emitSection() {
  const top = scrollTop.value;
  let cur: { label: string; y: number; h: number } | null = null;
  for (const row of layout.value.rows) {
    if (row.type === "header") {
      if (row.y <= top + 1) cur = { label: row.label, y: row.y, h: row.h };
      else break;
    }
  }
  const label = cur?.label ?? "";
  // 标题已滚出顶部才需要固定条；标题在视口内时不重复显示
  const pinned = !!cur && top > cur.y + cur.h - 2;
  const key = `${label}|${pinned ? "p" : "v"}`;
  if (key === lastSectionKey) return;
  lastSectionKey = key;
  emit("section", { label, pinned });
}

/** 接近底部时请求下一页（父组件的 loading 守卫去重）。 */
function maybeLoadMore() {
  if (!props.hasMore) return;
  const total = layout.value.totalH;
  const threshold = props.masonry ? viewportH.value * 1.5 : tileStride.value * 3;
  if (scrollTop.value + viewportH.value >= total - threshold) {
    emit("near-end");
  }
}

/* ---------- 滚轮滚动 ----------
 * 不自建动画状态机（曾因「原生滚动 / 自定义动画 / focus 滚动」三方写 scrollTop 反复出 bug），
 * 也不给容器加 `scroll-behavior: smooth`（那会让每次 `scrollTop =` 都触发动画，
 * 与逐帧赋值叠加 → 又顿又慢）。这里只把滚轮 delta 按缩放档位放大后**即时**赋值。
 * 触控板事件密集，即时赋值本身已足够顺滑；鼠标滚轮是离散步进。 */

/** 每格滚轮的基础位移倍率：单列最大，格子越密越小。 */
function wheelStep(): number {
  if (props.masonry) return 1.6;
  const c = props.columns;
  if (c <= 5) return 1.3;
  if (c <= 7) return 1.1;
  if (c <= 10) return 0.95;
  return 0.85;
}

/** 归一化不同设备的滚轮单位（行/页 → 像素）。 */
function normalizeDelta(e: WheelEvent): number {
  if (e.deltaMode === 1) return e.deltaY * 16;
  if (e.deltaMode === 2) return e.deltaY * (viewportH.value || 800);
  return e.deltaY;
}

function onWheel(e: WheelEvent) {
  if (e.ctrlKey) return; // Ctrl+滚轮留给缩放
  const el = scroller.value;
  if (!el) return;
  e.preventDefault();
  const max = Math.max(0, el.scrollHeight - el.clientHeight);
  const dest = Math.max(
    0,
    Math.min(max, el.scrollTop + normalizeDelta(e) * wheelStep()),
  );
  // 即时位移：不要每次滚轮都重启平滑动画（那会让动画永远被打断 → 又顿又慢）。
  // 触控板本身事件密集，即时位移已经足够顺滑；鼠标滚轮是离散步进。
  el.scrollTop = dest;
}

let ticking = false;
let lastScrollY = 0;
let lastBandLoad = 0;
function onScroll() {
  // 滚动期间与停稳后的短时间内都不触发悬停预览（设计 §7.3）。
  emit("suppress");

  // 快速拖动滚动条时一帧可能触发多次 scroll，用 rAF 节流到一帧一次重渲染。
  if (ticking) return;
  ticking = true;
  requestAnimationFrame(() => {
    const y = scroller.value?.scrollTop ?? 0;
    const velocity = Math.abs(y - lastScrollY);
    lastScrollY = y;
    scrollTop.value = y;
    ticking = false;
    emitSection();
    // 慢速（滚轮/平滑滚动）→ 预载视口外，避免灰块；节流到 ~80ms，
    // 否则每帧给上百个瓦片挂 src 会拖垮边缘帧率。
    // 快速（拖滚动条）→ 不加载，等停稳后一次性加载（已优化的路径）。
    const now = performance.now();
    if ((velocity < viewportH.value * 0.6 || settled.value) && now - lastBandLoad > 80) {
      lastBandLoad = now;
      markVisibleLoaded();
    }
    maybeLoadMore();
  });

  // 滚动途中只更新布局，不给新瓦片加载图片。
  settled.value = false;
  if (settleTimer !== undefined) clearTimeout(settleTimer);
  settleTimer = window.setTimeout(() => {
    markVisibleLoaded();
    settled.value = true;
    emitNeedLarge();
  }, 150);

  // 停稳后、过了 300ms 抑制窗口，补触发光标下那张的悬停预览
  if (hoverResumeTimer !== undefined) clearTimeout(hoverResumeTimer);
  hoverResumeTimer = window.setTimeout(resumeHoverUnderPointer, 330);
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
  const localX = clientX - rect.left + el.scrollLeft;
  const localY = clientY - rect.top + el.scrollTop;
  const viewportY = clientY - rect.top;

  for (const row of layout.value.rows) {
    if (row.type !== "tiles") {
      if (localY < row.y) {
        // 落在日期头/页脚里，锚到下一瓦片行第一格。
        const next = layout.value.rows.find((r) => r.type === "tiles" && r.y >= row.y);
        if (next && next.type === "tiles" && next.cells[0]) {
          return { index: next.cells[0].index, fy: 0, viewportY };
        }
      }
      continue;
    }
    // 直接命中格子（等边与瀑布流通用）
    for (const c of row.cells) {
      if (localX >= c.x && localX < c.x + c.w && localY >= c.y && localY < c.y + c.h) {
        return { index: c.index, fy: (localY - c.y) / c.h, viewportY };
      }
    }
  }
  return null;
}

function restoreAnchor(anchor: Anchor | null) {
  const el = scroller.value;
  if (!el || !anchor) return;
  const y = layout.value.indexToY(anchor.index);
  const h = layout.value.indexHeight(anchor.index);
  const dest = Math.max(0, y + anchor.fy * h - anchor.viewportY);
  // 平滑滑到锚点，避免换挡时生硬跳动（交给原生平滑滚动）。
  el.scrollTo({ top: dest, behavior: "smooth" });
}

let ro: ResizeObserver | null = null;
onMounted(() => {
  const el = scroller.value;
  if (!el) return;
  el.addEventListener("wheel", onWheel, { passive: false });
  el.addEventListener("mousemove", onMouseMove);
  el.addEventListener("mouseleave", onMouseLeave);
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
    emitNeedLarge();
    emitSection();
  });
});
onBeforeUnmount(() => {
  ro?.disconnect();
  if (settleTimer !== undefined) clearTimeout(settleTimer);
  clearPressTimer();
  stopListening();
  scroller.value?.removeEventListener("wheel", onWheel);
  scroller.value?.removeEventListener("mousemove", onMouseMove);
  scroller.value?.removeEventListener("mouseleave", onMouseLeave);
  if (reflowTimer !== undefined) clearTimeout(reflowTimer);
  if (hoverResumeTimer !== undefined) clearTimeout(hoverResumeTimer);
});

// 追加分页后高度增长、或视口变化时，检查是否需要继续加载（填满视口）。
watch([() => layout.value.totalH, viewportH], () => maybeLoadMore());

// 切到单列流时，为当前可见项请求高清大图。
watch(() => props.masonry, () => nextTick(emitNeedLarge));

// 分组/列数/粒度变化后刷新当前分段
watch(
  [() => props.groups, () => props.columns, () => props.granularity],
  () => nextTick(emitSection),
);

// 换挡过渡：列数变化时给整块内容一次轻微的淡出/回弹，避免生硬切换。
const reflowing = ref(false);
let reflowTimer: number | undefined;
watch(
  () => props.columns,
  () => {
    reflowing.value = true;
    if (reflowTimer !== undefined) clearTimeout(reflowTimer);
    reflowTimer = window.setTimeout(() => (reflowing.value = false), 220);
  },
);

defineExpose({ el: scroller, captureAnchor, restoreAnchor });
</script>

<template>
  <div ref="scroller" class="scroller" @scroll.passive="onScroll">
    <div
      class="canvas"
      :class="{ reflowing }"
      :style="{ height: layout.totalH + 'px' }"
    >
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
            :data-id="cell.asset.id"
            :data-index="cell.index"
            :tabindex="cell.index === tabbableIndex ? 0 : -1"
            role="button"
            :aria-pressed="props.selected.has(cell.asset.id)"
            :aria-label="cell.asset.base_name"
            :style="{
              width: cell.w + 'px',
              height: cell.h + 'px',
              '--ts': cell.w + 'px',
              transform: `translate(${cell.x}px, ${cell.y}px)`,
            }"
            @mouseenter="onTileEnter(cell, $event)"
            @mouseleave="emit('hover-out')"
            @pointerdown="onTilePointerDown(cell, $event)"
            @focus="focusedIndex = cell.index"
            @keydown="onTileKeydown(cell, $event)"
            @contextmenu="onTileContextMenu(cell, $event)"
          >
            <img
              :src="tileSrc(cell.index, pathOf(cell.asset))"
              :alt="cell.asset.base_name"
              loading="lazy"
              decoding="async"
              draggable="false"
            />
            <!-- 实况 / 视频徽标：固定角落，实况用 LIVE 文字（不用播放按钮，见 HIG live-photos） -->
            <span v-if="cell.asset.kind === 3" class="badge live">LIVE</span>
            <span v-else-if="cell.asset.kind === 2" class="badge">
              <AppIcon name="play" />
            </span>
            <span v-if="props.selected.has(cell.asset.id)" class="check">✓</span>
            <span v-else-if="armingId === cell.asset.id" class="check arming">
              <svg viewBox="0 0 24 24"><circle class="ring" cx="12" cy="12" r="10" /></svg>
            </span>
          </div>
        </template>
        <div
          v-else
          class="load-footer"
          :style="{ transform: `translateY(${row.y}px)` }"
        >
          <span v-if="props.loading">加载中…</span>
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
/* 焦点会立即转入瓦片，容器本身不画焦点环 */

.canvas {
  position: relative;
  width: 100%;
  transform-origin: top center;
  transition: opacity 0.22s ease, transform 0.22s cubic-bezier(0.2, 0.8, 0.2, 1);
}
/* 换挡过渡：列数变化瞬间轻微淡出/回弹，然后恢复 */
.canvas.reflowing {
  opacity: 0.25;
  transform: scale(0.99);
}
.date-header {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 0 2px;
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
  /* 相册网格：方角、几乎无缝（间隔由 gap=2 提供） */
  border-radius: 0;
  background: var(--tile);
  cursor: pointer;
  transition: box-shadow 0.16s;
}
/* 悬停用内描边而非外阴影：2px 间隔下阴影会盖到相邻图（HIG：避免频繁交互的动效） */
.tile:hover {
  box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--label) 55%, transparent);
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
/* 实况/视频徽标：固定左上角，字号随瓦片尺寸缩放 */
.badge {
  position: absolute;
  top: 6px;
  left: 6px;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 2px 5px;
  border-radius: 5px;
  background: rgba(0, 0, 0, 0.55);
  color: #fff;
  font-size: clamp(8px, calc(var(--ts, 96px) * 0.13), 11px);
  font-weight: 600;
  letter-spacing: 0.04em;
  line-height: 1;
  z-index: 1;
  pointer-events: none;
  backdrop-filter: blur(4px);
}
.badge.live::before {
  content: "";
  width: 0.5em;
  height: 0.5em;
  border-radius: 50%;
  background: currentColor;
  opacity: 0.85;
}
.badge svg {
  width: 1em;
  height: 1em;
}

/* 键盘焦点：外描边，明显区别于选中态 */
.tile:focus-visible {
  outline: 3px solid var(--accent);
  outline-offset: 2px;
  z-index: 3;
}
.check {
  position: absolute;
  /* 徽标随瓦片尺寸缩放：小瓦片（14 列）不会显得过大 */
  --cs: clamp(13px, calc(var(--ts, 96px) * 0.24), 24px);
  top: calc(var(--cs) * 0.25);
  right: calc(var(--cs) * 0.25);
  width: var(--cs);
  height: var(--cs);
  border-radius: 50%;
  background: var(--accent);
  color: #fff;
  font-size: calc(var(--cs) * 0.62);
  line-height: var(--cs);
  text-align: center;
  border: 2px solid #fff;
  box-sizing: border-box;
  z-index: 1;
}
/* 长按计时环：按住时右上角圆圈按 500ms 填满，填满即选中 */
.check.arming {
  background: rgba(0, 0, 0, 0.45);
  border-color: transparent;
}
.check.arming svg {
  width: 100%;
  height: 100%;
  transform: rotate(-90deg);
}
.check.arming .ring {
  fill: none;
  stroke: var(--accent);
  stroke-width: 3;
  stroke-dasharray: 63;
  stroke-dashoffset: 63;
  animation: ring-fill 0.5s linear forwards;
}
@keyframes ring-fill {
  to {
    stroke-dashoffset: 0;
  }
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
