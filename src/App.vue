<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary, type AssetRow } from "./stores/library";
import { useImport } from "./stores/import";
import { usePreview } from "./composables/usePreview";
import { useZoom } from "./composables/useZoom";
import { groupAssets, granularityForColumns, type Granularity } from "./lib/timeline";
import PhotoGrid from "./components/PhotoGrid.vue";
import PhotoViewer from "./components/PhotoViewer.vue";
import FilterBar from "./components/FilterBar.vue";
import AppIcon from "./components/AppIcon.vue";

const lib = useLibrary();
const imp = useImport();
const zoom = useZoom(5);
const grid = ref<InstanceType<typeof PhotoGrid> | null>(null);
const main = ref<HTMLElement | null>(null);
const assumeCloud = ref(false);

/* ---------- Toast ---------- */
type Toast = { id: number; text: string; kind: "info" | "error"; copy?: string };
const toasts = ref<Toast[]>([]);
let toastSeq = 0;
function toast(text: string, kind: "info" | "error" = "info", copy?: string) {
  const id = ++toastSeq;
  toasts.value = [...toasts.value, { id, text, kind, copy }];
  // accessibility.md:96：错误提示不自动消失，避免用户错过；信息提示 4s 后收起。
  if (kind === "info") {
    window.setTimeout(() => dismissToast(id), 4000);
  }
}
function dismissToast(id: number) {
  toasts.value = toasts.value.filter((t) => t.id !== id);
}
function onToastClick(t: Toast) {
  if (t.copy) void copyText(t.copy);
  else dismissToast(t.id);
}
function toastError() {
  if (lib.error) {
    toast(lib.error, "error");
    lib.error = null;
  }
}

/** 点击复制（桌面 app 里替代可框选文本）。 */
async function copyText(text: string) {
  try {
    if (!navigator.clipboard?.writeText) throw new Error("no clipboard api");
    await navigator.clipboard.writeText(text);
    toast("已复制");
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    toast(ok ? "已复制" : "复制失败", ok ? "info" : "error");
  }
}

/* ---------- 确认框（替换 window.confirm） ---------- */
const confirmMsg = ref<string | null>(null);
let confirmResolve: ((v: boolean) => void) | null = null;
function askConfirm(msg: string): Promise<boolean> {
  confirmMsg.value = msg;
  return new Promise((res) => (confirmResolve = res));
}
function answerConfirm(v: boolean) {
  confirmMsg.value = null;
  confirmResolve?.(v);
  confirmResolve = null;
}

/* ---------- 动作 ---------- */
async function pickLibrary() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === "string") {
    await lib.openLibrary(dir);
    toastError();
  }
}

async function importFromDevice() {
  await imp.start(assumeCloud.value);
  await lib.refresh();
  if (imp.error) {
    toast(imp.error, "error");
    return;
  }
  const p = imp.progress;
  if (!p) return;
  if (p.cancelled) {
    toast(`已取消（${p.done}/${p.total}）`);
  } else {
    toast(
      `导入完成 ${p.done}/${p.total}${p.failed ? ` · 失败 ${p.failed}` : ""}`,
      p.failed ? "error" : "info",
    );
  }
}

async function doRescan() {
  await lib.rescan();
  if (lib.error) toastError();
  // 成功由侧栏进度条体现，不再弹 toast（feedback.md：预期成功不必确认）
}

async function doThumbs() {
  await lib.generateThumbs();
  if (lib.error) toastError();
  else if (lib.thumbs?.failed) {
    toast(`缩略图失败 ${lib.thumbs.failed} 项`, "error");
  }
}

async function doClassify() {
  const cancelled = await lib.classify(assumeCloud.value);
  if (lib.error) toastError();
  else if (cancelled) toast("已取消校验");
}

async function exportDiag() {
  try {
    const path = await lib.exportDiagnostics();
    toast(`诊断报告已生成：${path}`, "info", path);
  } catch (e) {
    toast(String(e), "error");
  }
}

async function exportIdsTo(ids: number[]) {
  if (!ids.length) return;
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir !== "string") return;
  await lib.exportIds(ids, dir);
  if (lib.error) toastError();
  else toast(`已导出 ${ids.length} 个条目到 ${dir}`, "info", dir);
}
async function exportSelected() {
  await exportIdsTo([...lib.selected]);
}

async function deleteIdsConfirm(ids: number[]) {
  if (!ids.length) return;
  // feedback.md：预期内的删除不必警告（Finder 移入废纸篓不弹确认）；仅批量时确认。
  if (ids.length >= 10) {
    const ok = await askConfirm(
      `将删除 ${ids.length} 个条目并移入回收站（实况条目会同时删除静态图与视频）。\n回收站容量不足时，大文件可能被永久删除。`,
    );
    if (!ok) return;
  }
  await lib.deleteIds(ids);
  if (lib.error) toastError();
  // 成功不弹 toast：条目消失本身就是反馈
}
async function deleteSelected() {
  await deleteIdsConfirm([...lib.selected]);
}

/* ---------- 网格与预览 ---------- */
const videoEl = ref<HTMLVideoElement | null>(null);
const pv = usePreview(videoEl);
const hoverRect = ref<{ x: number; y: number; w: number; h: number } | null>(null);

const gridAssets = computed<AssetRow[]>(() =>
  lib.assets.filter((a) => !a.missing && (a.thumb_path || a.still_path)),
);

const gran = computed<Granularity>(() =>
  lib.granOverride === "auto" ? granularityForColumns(zoom.columns.value) : lib.granOverride,
);
const groups = computed(() => groupAssets(gridAssets.value, gran.value));
// 最大一档（1 列）是单列大图流；3 列仍是普通网格。
const masonry = computed(() => zoom.columns.value <= 1);

/* ---------- 单列流：按需生成高清大图 ---------- */
const largeById = ref<Record<number, string>>({});
const largeInflight = new Set<number>();
let largeActive = 0;
const MAX_LARGE = 2;

async function ensureLarge(id: number) {
  if (!masonry.value) return;
  if (largeById.value[id] || largeInflight.has(id)) return;
  if (largeActive >= MAX_LARGE) return; // 限流：下次可见再试
  largeInflight.add(id);
  largeActive++;
  try {
    const p = await invoke<string>("ensure_large", { assetId: id });
    largeById.value = { ...largeById.value, [id]: p };
  } catch {
    /* 生成失败就退回缩略图 */
  } finally {
    largeInflight.delete(id);
    largeActive--;
  }
}
function onNeedLarge(ids: number[]) {
  for (const id of ids) void ensureLarge(id);
}

function onNearEnd() {
  void lib.loadMore();
}
function onToggleSelect(id: number) {
  lib.toggleSelect(id);
}
function onSetSelected(id: number, value: boolean) {
  lib.setSelection([id], value);
}

/* ---------- 顶部固定分段条 ---------- */
const section = ref<{ label: string; pinned: boolean }>({ label: "", pinned: false });

/* ---------- 单张查看 ---------- */
const viewerIndex = ref<number | null>(null);
function openViewer(id: number) {
  const i = gridAssets.value.findIndex((a) => a.id === id);
  if (i < 0) return;
  pv.hide();
  hoverRect.value = null;
  viewerIndex.value = i;
}

const slotStyle = computed(() => {
  const r = hoverRect.value;
  if (!r) return {};
  return { left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px` };
});

function onHover(payload: { asset: AssetRow | null; rect: DOMRect }) {
  const base = main.value?.getBoundingClientRect();
  if (!base) return;
  hoverRect.value = {
    x: payload.rect.left - base.left,
    y: payload.rect.top - base.top,
    w: payload.rect.width,
    h: payload.rect.height,
  };
  pv.enter(payload.asset);
}
function onHoverOut() {
  pv.hide();
  hoverRect.value = null;
}
function onSuppress() {
  pv.suppress();
  hoverRect.value = null;
  ctx.value = null; // 滚动时收起右键菜单
}
function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return;
  e.preventDefault();
  pv.suppress();
  const anchor = grid.value?.captureAnchor(e.clientX, e.clientY) ?? null;
  const dpr = window.devicePixelRatio || 1;
  const vw = main.value?.clientWidth ?? window.innerWidth;
  zoom.columns.value = zoom.next(e.deltaY, vw, dpr);
  nextTick(() => grid.value?.restoreAnchor(anchor));
}
/** 全局禁止原生拖拽（图片/链接/文本），需要拖拽的交互再单独放行。 */
function preventDrag(e: DragEvent) {
  e.preventDefault();
}

/** 全局禁用原生右键菜单；仅文本输入/可编辑区域放行（复制粘贴需要）。 */
function preventContextMenu(e: MouseEvent) {
  const t = e.target as HTMLElement | null;
  if (t?.closest("input, textarea, [contenteditable='true']")) return;
  e.preventDefault();
}

/* ---------- 侧栏「更多」菜单 ---------- */
const menuOpen = ref(false);
const menuEl = ref<HTMLElement | null>(null);

function menuItems(): HTMLButtonElement[] {
  return Array.from(
    menuEl.value?.querySelectorAll<HTMLButtonElement>(".menu-item:not(:disabled)") ?? [],
  );
}
/** 打开菜单时聚焦第一项；方向键在项间移动，Esc 关闭（menus.md / focus-and-selection.md）。 */
watch(menuOpen, async (open) => {
  if (!open) return;
  await nextTick();
  menuItems()[0]?.focus();
});
function onMenuKeydown(e: KeyboardEvent) {
  const items = menuItems();
  if (!items.length) return;
  const cur = items.indexOf(document.activeElement as HTMLButtonElement);
  if (e.key === "ArrowDown") {
    e.preventDefault();
    items[(cur + 1) % items.length]?.focus();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    items[(cur - 1 + items.length) % items.length]?.focus();
  } else if (e.key === "Home") {
    e.preventDefault();
    items[0]?.focus();
  } else if (e.key === "End") {
    e.preventDefault();
    items[items.length - 1]?.focus();
  } else if (e.key === "Escape") {
    e.preventDefault();
    menuOpen.value = false;
  }
}
const granOptions: { label: string; value: "auto" | "year" | "month" | "day" }[] = [
  { label: "自动", value: "auto" },
  { label: "年", value: "year" },
  { label: "月", value: "month" },
  { label: "天", value: "day" },
];
function runMenu(fn: () => unknown) {
  menuOpen.value = false;
  void fn();
}
function onDocClick() {
  menuOpen.value = false;
  ctx.value = null;
}
function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    menuOpen.value = false;
    ctx.value = null;
  }
}

/* ---------- 瓦片右键菜单（只放主界面已有的动作；删除置底红色） ----------
 * 右键**不改变选择**：作用于「被点条目」，或（若它已在选区里）整个选区。 */
const ctx = ref<{ x: number; y: number } | null>(null);
const ctxIds = ref<number[]>([]);
function onContextMenu(id: number, x: number, y: number) {
  ctxIds.value = lib.selected.has(id) ? [...lib.selected] : [id];
  ctx.value = {
    x: Math.min(x, window.innerWidth - 200),
    y: Math.min(y, window.innerHeight - 150),
  };
}
function ctxExport() {
  const ids = ctxIds.value;
  ctx.value = null;
  void exportIdsTo(ids);
}
function ctxDelete() {
  const ids = ctxIds.value;
  ctx.value = null;
  void deleteIdsConfirm(ids);
}

async function openRecent(path: string) {
  await lib.openLibrary(path);
  if (lib.error) {
    toast(lib.error, "error");
    lib.error = null;
  }
}
async function forgetRecent(path: string) {
  await lib.forgetRecent(path);
}

onMounted(() => {
  document.addEventListener("dragstart", preventDrag);
  document.addEventListener("contextmenu", preventContextMenu);
  document.addEventListener("click", onDocClick);
  document.addEventListener("keydown", onKeydown);
  void lib.loadRecents();
});
onBeforeUnmount(() => {
  document.removeEventListener("dragstart", preventDrag);
  document.removeEventListener("contextmenu", preventContextMenu);
  document.removeEventListener("click", onDocClick);
  document.removeEventListener("keydown", onKeydown);
});

/* ---------- 统一进度区（侧栏一处，progress-indicators.md：位置固定） ---------- */
const importPct = computed(() => {
  const p = imp.progress;
  return p && p.total ? Math.round((p.done / p.total) * 100) : 0;
});
const importMB = computed(() => ((imp.progress?.bytes_done ?? 0) / 1048576).toFixed(1));
const thumbPct = computed(() => {
  const t = lib.thumbs;
  return t && t.total ? Math.round((t.done / t.total) * 100) : 0;
});
const classifyPct = computed(() => {
  const c = lib.classifyProgress;
  return c && c.total ? Math.round((c.done / c.total) * 100) : 0;
});

/** 当前正在进行的长任务（同一时刻只有一个），供侧栏进度区展示。 */
type ActiveTask =
  | { kind: "import"; pct: number; text: string; right: string; cancellable: true }
  | { kind: "scan"; pct: number | null; text: string; right: string; cancellable: true }
  | { kind: "thumbs"; pct: number; text: string; right: string; cancellable: false }
  | { kind: "classify"; pct: number; text: string; right: string; cancellable: true }
  | { kind: "busy"; pct: number | null; text: string; right: string; cancellable: false };

const activeTask = computed<ActiveTask | null>(() => {
  if (imp.running) {
    const p = imp.progress;
    return {
      kind: "import",
      pct: importPct.value,
      text: `导入中 ${p?.done ?? 0}/${p?.total ?? "?"}`,
      right: `${importMB.value} MB`,
      cancellable: true,
    };
  }
  if (lib.scanning) {
    const s = lib.scanProgress;
    return {
      kind: "scan",
      pct: null, // 扫描总量未知 → 不确定型
      text: `重建索引… ${s?.files ?? 0} 个文件`,
      right: `${s?.assets ?? 0} 条目`,
      cancellable: true,
    };
  }
  if (lib.classifyProgress) {
    const c = lib.classifyProgress;
    return {
      kind: "classify",
      pct: classifyPct.value,
      text: `校验标识 ${c.done}/${c.total}`,
      right: "",
      cancellable: true,
    };
  }
  if (lib.thumbs) {
    return {
      kind: "thumbs",
      pct: thumbPct.value,
      text: `缩略图 ${lib.thumbs.done}/${lib.thumbs.total}`,
      right: lib.thumbs.failed ? `失败 ${lib.thumbs.failed}` : "",
      cancellable: false,
    };
  }
  if (lib.busy) {
    return { kind: "busy", pct: null, text: "处理中…", right: "", cancellable: false };
  }
  return null;
});

function cancelActive() {
  const t = activeTask.value;
  if (!t) return;
  if (t.kind === "import") imp.stop();
  else if (t.kind === "scan") lib.cancelScan();
  else if (t.kind === "classify") lib.cancelScan();
}

const stats = computed(() => lib.stats);
const abnormal = computed(() => {
  const s = lib.stats;
  if (!s) return 0;
  return s.by_integrity.filter(([k]) => [1, 2, 5].includes(k)).reduce((a, [, n]) => a + n, 0);
});
const noResults = computed(
  () => !!lib.root && !lib.loading && gridAssets.value.length === 0,
);
const filtersActive = computed(
  () =>
    !!lib.filter.text.trim() ||
    lib.filter.kind !== null ||
    lib.filter.integrity.length > 0 ||
    !!lib.filter.from ||
    !!lib.filter.to,
);
</script>

<template>
  <!-- ================= 欢迎 / 选择资料库（无库时整屏） ================= -->
  <div v-if="!lib.root" class="welcome">
    <div class="welcome-inner">
      <div class="hero">
        <div class="hero-logo">L</div>
        <h1>LivePorter</h1>
        <p>把 iPhone 的实况照片原样搬进硬盘</p>
      </div>

      <template v-if="lib.recents.length">
        <div class="group-title">最近</div>
        <ul class="group-list">
          <li
            v-for="r in lib.recents"
            :key="r.path"
            class="group-row"
            :class="{ gone: !r.exists }"
          >
            <button class="row-main" :disabled="!r.exists" @click="openRecent(r.path)">
              <span class="row-icon"><AppIcon name="folder" :size="16" /></span>
              <span class="row-text">
                <span class="row-name">{{ r.name }}</span>
                <span class="row-sub">{{ r.exists ? r.path : r.path + " · 位置不存在" }}</span>
              </span>
              <span class="row-chevron">›</span>
            </button>
            <button class="row-remove" title="移除" @click="forgetRecent(r.path)">✕</button>
          </li>
        </ul>
        <button class="btn prominent welcome-cta" @click="pickLibrary">
          打开其他资料库…
        </button>
      </template>

      <template v-else>
        <ol class="steps">
          <li>
            <span class="step-n">1</span>
            <div><b>打开资料库</b><span>选一个硬盘目录存放照片</span></div>
          </li>
          <li>
            <span class="step-n">2</span>
            <div><b>iPhone 设为「保留原件」</b><span>设置 → 照片 → 传输到 Mac 或 PC</span></div>
          </li>
          <li>
            <span class="step-n">3</span>
            <div><b>从 iPhone 导入</b><span>插上线即可，支持断点续传</span></div>
          </li>
        </ol>
        <button class="btn prominent welcome-cta" @click="pickLibrary">打开资料库…</button>
      </template>
    </div>
  </div>

  <div v-else class="app">
    <!-- ================= 侧栏 ================= -->
    <aside class="sidebar">
      <div class="brand">
        <div class="logo">L</div>
        <div class="brand-text">
          <h1>LivePorter</h1>
          <div class="sub">iPhone 实况照片搬运</div>
        </div>
        <button
          class="icon-btn"
          title="更多"
          aria-label="更多操作"
          :aria-expanded="menuOpen"
          @click.stop="menuOpen = !menuOpen"
        >
          ⋯
        </button>
        <div
          v-if="menuOpen"
          ref="menuEl"
          class="menu"
          role="menu"
          tabindex="-1"
          @click.stop
          @keydown="onMenuKeydown"
        >
          <button class="menu-item" :disabled="!lib.root || lib.busy" @click="runMenu(doRescan)">
            重建索引
          </button>
          <button class="menu-item" :disabled="!lib.root || lib.busy" @click="runMenu(doThumbs)">
            生成缩略图
          </button>
          <button class="menu-item" :disabled="!lib.root || lib.busy" @click="runMenu(doClassify)">
            校验标识
          </button>
          <div class="menu-sep"></div>
          <button class="menu-item" :disabled="!lib.root" @click="runMenu(exportDiag)">
            导出诊断报告…
          </button>
          <div class="menu-sep"></div>
          <div class="menu-label">时间线分段</div>
          <button
            v-for="g in granOptions"
            :key="g.value"
            class="menu-item"
            @click="lib.granOverride = g.value; menuOpen = false"
          >
            <span class="menu-check">{{ lib.granOverride === g.value ? "✓" : "" }}</span>
            {{ g.label }}
          </button>
        </div>
      </div>

      <div class="sec">
        <div class="sec-title">资料库</div>
        <div class="card">
          <button
            class="libpath"
            :title="lib.root ? '点击复制路径' : ''"
            :disabled="!lib.root"
            @click="lib.root && copyText(lib.root)"
          >
            <AppIcon name="folder" :size="14" />
            <span>{{ lib.root || "未打开资料库" }}</span>
          </button>
          <button class="btn block" style="margin-top: 10px" @click="pickLibrary">
            打开资料库…
          </button>
          <div v-if="stats" class="stats">
            <div class="stat"><b>{{ stats.total.toLocaleString() }}</b><span>总数</span></div>
            <div class="stat"><b>{{ stats.live.toLocaleString() }}</b><span>实况</span></div>
            <div class="stat"><b>{{ stats.photo.toLocaleString() }}</b><span>照片</span></div>
            <div class="stat"><b>{{ stats.video.toLocaleString() }}</b><span>视频</span></div>
            <div class="stat"><b>{{ stats.missing.toLocaleString() }}</b><span>缺失</span></div>
            <div class="stat" :class="{ warn: abnormal > 0 }">
              <b>{{ abnormal }}</b><span>异常</span>
            </div>
          </div>
        </div>
      </div>

      <div class="sec">
        <div class="sec-title">导入</div>
        <button
          class="btn prominent block"
          :disabled="!lib.root || imp.running"
          @click="importFromDevice"
        >
          <span v-if="imp.running" class="spinner-sm" aria-hidden="true"></span>
          <AppIcon v-else name="iphone" />{{ imp.running ? "导入中…" : "从 iPhone 导入" }}
        </button>
        <label class="switch" :class="{ on: assumeCloud }">
          <input
            type="checkbox"
            role="switch"
            class="switch-input"
            v-model="assumeCloud"
          />
          <span class="track"><i></i></span>原件可能不在手机
        </label>
      </div>

      <div class="spacer"></div>

      <!-- 统一进度区（所有长任务都在这里显示） -->
      <div v-if="activeTask" class="taskbar">
        <div class="progress">
          <i v-if="activeTask.pct !== null" :style="{ width: activeTask.pct + '%' }"></i>
          <i v-else class="indeterminate"></i>
        </div>
        <div class="ptext">
          <span>{{ activeTask.text }}</span>
          <span v-if="activeTask.right">{{ activeTask.right }}</span>
        </div>
        <button v-if="activeTask.cancellable" class="btn block" style="margin-top: 8px" @click="cancelActive">
          取消
        </button>
      </div>
      <div class="sidefoot">v1.0.0 · GPL-3.0 · ffmpeg 9.0.1</div>
    </aside>

    <!-- ================= 主区 ================= -->
    <section class="main">
      <div class="toolbar">
        <span class="title">资料库</span>
        <FilterBar />
      </div>

      <div class="stage" ref="main" @wheel="onWheel">
        <PhotoGrid
          ref="grid"
          :groups="groups"
          :columns="zoom.columns.value"
          :gap="2"
          :granularity="gran"
          :selected="lib.selected"
          :has-more="lib.assets.length < lib.total"
          :loading="lib.loading"
          :masonry="masonry"
          :large-by-id="largeById"
          @hover="onHover"
          @hover-out="onHoverOut"
          @suppress="onSuppress"
          @toggle-select="onToggleSelect"
          @set-selected="onSetSelected"
          @need-large="onNeedLarge"
          @near-end="onNearEnd"
          @context-menu="onContextMenu"
          @request-delete="deleteSelected"
          @open="openViewer"
          @section="section = $event"
        />

        <!-- 顶部固定分段条：本段标题滚出顶部时钉住显示 -->
        <div v-if="section.pinned && section.label" class="section-bar">
          {{ section.label }}
        </div>

        <!-- 状态 HUD：移出工具栏，数字变化不再推动其它控件（HIG：控件位置保持稳定） -->
        <div class="hud">
          <span>{{ lib.total.toLocaleString() }} 项</span>
          <span class="hud-sep">·</span>
          <span>{{ masonry ? "单列" : zoom.columns.value + " 列" }}</span>
        </div>

        <!-- 筛选无结果 / 空库 -->
        <div v-if="noResults" class="empty">
          <div class="empty-card">
            <h2>{{ filtersActive ? "没有匹配项" : "资料库还是空的" }}</h2>
            <p class="muted">
              {{
                filtersActive
                  ? "换个关键词或清空筛选试试。"
                  : "插上 iPhone，点侧栏的「从 iPhone 导入」。"
              }}
            </p>
            <button v-if="filtersActive" class="btn" @click="lib.clearFilters">
              清空筛选
            </button>
          </div>
        </div>

        <!-- 全局唯一一个 <video>，绝对定位到当前悬停的瓦片上 -->
        <div v-show="hoverRect" class="preview-slot" :style="slotStyle">
          <video
            ref="videoEl"
            class="preview"
            :class="{ show: pv.visible }"
            muted
            playsinline
            loop
          ></video>
          <span v-if="pv.loading" class="preview-loading"></span>
        </div>
      </div>
    </section>

    <!-- ================= 选中操作条 ================= -->
    <div v-if="lib.selected.size" class="selbar">
      <span class="count">已选 <b>{{ lib.selected.size }}</b></span>
      <button class="btn plain" @click="lib.selectAll">全选</button>
      <button class="btn plain" @click="lib.clearSelection">清除</button>
      <button class="btn plain" :disabled="lib.exporting" @click="exportSelected">
        <AppIcon name="export" />导出
      </button>
      <button class="btn destructive" :disabled="lib.deleting" @click="deleteSelected">
        <AppIcon name="trash" />删除
      </button>
    </div>

    <!-- ================= Toast ================= -->
    <div class="toasts" aria-live="polite">
      <div
        v-for="t in toasts"
        :key="t.id"
        class="toast"
        :class="[t.kind, 'clickable']"
        :role="t.kind === 'error' ? 'alert' : 'status'"
        :title="t.copy ? '点击复制' : '点击关闭'"
        @click="onToastClick(t)"
      >
        <AppIcon :name="t.kind === 'error' ? 'warn' : 'check'" :size="15" />
        <span>{{ t.text }}</span>
      </div>
    </div>

    <!-- ================= 瓦片右键菜单 ================= -->
    <div
      v-if="ctx"
      class="ctxmenu"
      :style="{ left: ctx.x + 'px', top: ctx.y + 'px' }"
      @click.stop
    >
      <div v-if="ctxIds.length > 1" class="ctx-title">
        已选 {{ ctxIds.length }} 项
      </div>
      <button class="menu-item" @click="ctxExport">
        <AppIcon name="export" :size="15" />导出…
      </button>
      <div class="menu-sep"></div>
      <button class="menu-item danger" @click="ctxDelete">
        <AppIcon name="trash" :size="15" />删除…
      </button>
    </div>

    <!-- ================= 单张查看 ================= -->
    <PhotoViewer
      v-if="viewerIndex !== null"
      :assets="gridAssets"
      :index="viewerIndex"
      @close="viewerIndex = null"
      @navigate="viewerIndex = $event"
    />

    <!-- ================= 确认框 ================= -->
    <div v-if="confirmMsg" class="modal-backdrop" @click.self="answerConfirm(false)">
      <div class="modal" role="dialog" aria-modal="true">
        <div class="modal-icon"><AppIcon name="warn" :size="22" /></div>
        <p class="modal-text">{{ confirmMsg }}</p>
        <div class="modal-actions">
          <button class="btn plain" @click="answerConfirm(false)">取消</button>
          <button class="btn destructive" @click="answerConfirm(true)">删除</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style>
.app {
  display: grid;
  grid-template-columns: 260px 1fr;
  height: 100vh;
}

/* 侧栏：材质较实、层次最高 */
.sidebar {
  background: var(--material-strong);
  border-right: 1px solid var(--separator);
  backdrop-filter: blur(24px) saturate(160%);
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.brand {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 18px 16px 12px;
}
.logo {
  width: 32px;
  height: 32px;
  border-radius: 9px;
  background: linear-gradient(135deg, var(--accent), #7a6bff);
  display: grid;
  place-items: center;
  font-weight: 700;
  color: #fff;
  font-size: 15px;
}
.brand h1 {
  font-size: 15px;
  font-weight: 650;
  margin: 0;
}
.brand .sub {
  font-size: 12px;
  color: var(--label-2);
}
.brand {
  position: relative;
}
.brand-text {
  flex: 1;
  min-width: 0;
}
.icon-btn {
  width: 30px;
  height: 30px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--label-2);
  font-size: 18px;
  line-height: 1;
}
.icon-btn:hover {
  background: var(--fill);
  color: var(--label);
}

/* 「更多」下拉菜单 */
.menu {
  position: absolute;
  top: 52px;
  right: 12px;
  min-width: 180px;
  padding: 6px;
  background: var(--card);
  border: 1px solid var(--separator);
  border-radius: 12px;
  box-shadow: var(--shadow);
  z-index: 40;
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  text-align: left;
  border: 0;
  background: transparent;
  color: var(--label);
  font-size: 13px;
  padding: 8px 10px;
  border-radius: 8px;
}
.menu-item:hover:not(:disabled) {
  background: var(--fill);
}
.menu-item:disabled {
  opacity: 0.4;
  cursor: default;
}
.menu-item.danger {
  color: var(--danger);
}
.menu-item.danger:hover {
  background: color-mix(in srgb, var(--danger) 14%, transparent);
}
.menu-sep {
  height: 1px;
  background: var(--separator);
  margin: 6px 4px;
}
.menu-label {
  font-size: 11px;
  color: var(--label-2);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: 4px 10px 2px;
}
.menu-check {
  display: inline-block;
  width: 14px;
  color: var(--accent);
}

/* 瓦片右键菜单 */
.ctxmenu {
  position: fixed;
  min-width: 184px;
  padding: 6px;
  background: var(--card);
  border: 1px solid var(--separator);
  border-radius: 12px;
  box-shadow: var(--shadow);
  z-index: 45;
}
.ctx-title {
  font-size: 11px;
  color: var(--label-3);
  padding: 6px 10px 4px;
}

/* 侧栏底部统一进度区 */
.taskbar {
  padding: 10px 14px 12px;
  border-top: 1px solid var(--separator);
}
.taskbar .progress {
  position: relative;
  overflow: hidden;
}
/* 不确定型：来回滑动的细条 */
.progress .indeterminate {
  position: absolute;
  top: 0;
  left: 0;
  width: 30%;
  animation: scan-slide 1.1s ease-in-out infinite;
}
@keyframes scan-slide {
  0% {
    left: -30%;
  }
  100% {
    left: 100%;
  }
}
/* 按钮内活动指示 */
.spinner-sm {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.45);
  border-top-color: #fff;
  animation: spin 0.7s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* 欢迎 / 选择资料库 */
.welcome {
  height: 100vh;
  display: grid;
  place-items: center;
  padding: 24px;
  background: var(--bg);
  overflow: auto;
}
.welcome-inner {
  width: min(520px, 92vw);
}
.hero {
  text-align: center;
  margin-bottom: 26px;
}
.hero-logo {
  width: 56px;
  height: 56px;
  border-radius: 15px;
  margin: 0 auto 14px;
  background: linear-gradient(135deg, var(--accent), #7a6bff);
  display: grid;
  place-items: center;
  font-weight: 700;
  color: #fff;
  font-size: 26px;
  box-shadow: 0 8px 22px color-mix(in srgb, var(--accent) 35%, transparent);
}
.hero h1 {
  font-size: 28px;
  font-weight: 650;
  letter-spacing: -0.02em;
  margin: 0 0 6px;
}
.hero p {
  margin: 0;
  color: var(--label-2);
  font-size: 15px;
}

.group-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--label-2);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin: 0 6px 8px;
}
.group-list {
  list-style: none;
  margin: 0 0 18px;
  padding: 0;
  background: var(--card);
  border: 1px solid var(--separator);
  border-radius: 16px;
  overflow: hidden;
}
.group-row {
  display: flex;
  align-items: stretch;
}
.group-row + .group-row {
  border-top: 1px solid var(--separator);
}
.row-main {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  background: transparent;
  border: 0;
  text-align: left;
}
.row-main:hover:not(:disabled) {
  background: var(--fill);
}
.row-main:disabled {
  opacity: 0.45;
  cursor: default;
}
.row-icon {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  flex: none;
  background: var(--fill);
  color: var(--accent);
  display: grid;
  place-items: center;
}
.row-text {
  display: grid;
  gap: 1px;
  min-width: 0;
}
.row-name {
  font-size: 14px;
  font-weight: 600;
}
.row-sub {
  font-size: 12px;
  color: var(--label-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.group-row.gone .row-name {
  color: var(--label-2);
}
.row-chevron {
  color: var(--label-2);
  font-size: 18px;
  margin-left: auto;
}
.row-remove {
  width: 42px;
  border: 0;
  background: transparent;
  color: var(--label-3);
  opacity: 0;
  transition: opacity 0.12s;
}
.group-row:hover .row-remove {
  opacity: 1;
}
.row-remove:hover {
  color: var(--danger);
}
.welcome-cta {
  width: 100%;
  min-height: 40px;
}

/* 首次引导步骤 */
.steps {
  list-style: none;
  margin: 0 0 22px;
  padding: 0;
  display: grid;
  gap: 14px;
}
.steps li {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}
.step-n {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  flex: none;
  background: var(--fill);
  color: var(--label);
  display: grid;
  place-items: center;
  font-size: 13px;
  font-weight: 600;
}
.steps b {
  display: block;
  font-size: 14px;
  font-weight: 600;
}
.steps li div span {
  font-size: 12px;
  color: var(--label-2);
}
.sec {
  padding: 4px 12px 12px;
}
.sec-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--label-2);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 8px 6px 6px;
}
.libpath {
  display: flex;
  gap: 7px;
  align-items: flex-start;
  width: 100%;
  background: none;
  border: 0;
  padding: 0;
  text-align: left;
  font-size: 12px;
  color: var(--label-2);
  word-break: break-all;
  line-height: 1.35;
  cursor: pointer;
}
.libpath:hover:not(:disabled) {
  color: var(--label);
}
.libpath:disabled {
  cursor: default;
}
.stats {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 8px;
  margin-top: 12px;
}
.stat {
  background: var(--fill);
  border-radius: var(--r-ctl);
  padding: 7px 8px;
}
.stat b {
  display: block;
  font-size: 16px;
  font-weight: 650;
}
.stat span {
  font-size: 11px;
  color: var(--label-2);
}
.stat.warn b {
  color: var(--danger);
}
.ptext {
  font-size: 12px;
  color: var(--label-2);
  margin-top: 6px;
  display: flex;
  justify-content: space-between;
}
.sidefoot {
  padding: 12px 16px;
  border-top: 1px solid var(--separator);
  font-size: 11px;
  color: var(--label-2);
}

/* 主区 */
.main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
.toolbar {
  position: sticky;
  top: 0;
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 10px 16px;
  background: var(--material);
  backdrop-filter: blur(24px) saturate(160%);
  border-bottom: 1px solid var(--separator);
}
.title {
  font-size: 17px;
  font-weight: 650;
  margin-right: 2px;
}
/* 顶部固定分段条：仅当本段标题滚出顶部时出现，避免与内联标题重复 */
.section-bar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 3;
  pointer-events: none;
  padding: 10px 16px;
  font-size: 17px;
  font-weight: 650;
  letter-spacing: -0.01em;
  color: var(--label);
  /* 半透明材质带：内容从下方滚过时仍可读（scroll-views：scroll edge effect 只用于控件与内容交界） */
  background: linear-gradient(180deg, var(--bg) 55%, color-mix(in srgb, var(--bg) 60%, transparent) 80%, transparent);
  backdrop-filter: blur(10px) saturate(140%);
}

/* 状态 HUD：右下角浮层，不参与工具栏布局，数字变化不影响其它控件 */
.hud {
  position: absolute;
  right: 12px;
  bottom: 10px;
  z-index: 4;
  pointer-events: none;
  display: flex;
  gap: 6px;
  padding: 4px 10px;
  border-radius: 999px;
  background: var(--material);
  backdrop-filter: blur(20px) saturate(160%);
  border: 1px solid var(--separator);
  color: var(--label-2);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.hud-sep {
  color: var(--label-3);
}
.stage {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

/* 空状态 */
.empty {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.empty-card {
  pointer-events: auto;
  text-align: center;
  max-width: 340px;
  padding: 28px;
  background: var(--card);
  border: 1px solid var(--separator);
  border-radius: var(--r-card);
  box-shadow: var(--shadow);
}
.empty-card h2 {
  font-size: 17px;
  margin: 0 0 6px;
}
.empty-card p {
  margin: 0 0 16px;
  font-size: 13px;
}
.empty-card .btn {
  margin: 0 auto;
}

/* 悬停预览 */
.preview-slot {
  position: absolute;
  pointer-events: none;
  z-index: 2;
}
.preview {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  opacity: 0;
  transition: opacity 120ms linear;
  border-radius: 0;
}
.preview.show {
  opacity: 1;
}
.preview-loading {
  position: absolute;
  right: 8px;
  bottom: 8px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #fff;
  opacity: 0.75;
  animation: preview-pulse 900ms ease-in-out infinite;
}
@keyframes preview-pulse {
  0%,
  100% {
    opacity: 0.25;
  }
  50% {
    opacity: 0.9;
  }
}

/* 选中操作条 */
.selbar {
  position: fixed;
  left: calc(260px + (100vw - 260px) / 2);
  bottom: 24px;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  background: var(--material);
  backdrop-filter: blur(24px) saturate(160%);
  border: 1px solid var(--separator);
  border-radius: 16px;
  box-shadow: var(--shadow);
  z-index: 10;
  white-space: nowrap;
}
.selbar .count {
  font-size: 13px;
  color: var(--label-2);
  padding: 0 6px;
}
.selbar .count b {
  color: var(--label);
}
.selbar .btn {
  width: auto;
}

/* Toast */
.toasts {
  position: fixed;
  right: 18px;
  bottom: 18px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 30;
  max-width: 420px;
}
.toast {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  background: var(--material);
  backdrop-filter: blur(24px) saturate(160%);
  border: 1px solid var(--separator);
  border-radius: 12px;
  box-shadow: var(--shadow);
  font-size: 13px;
}
.toast.error {
  color: var(--danger);
}
.toast.clickable {
  cursor: pointer;
}

/* 确认框 */
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: grid;
  place-items: center;
  z-index: 40;
}
.modal {
  width: min(360px, 88vw);
  background: var(--card);
  border-radius: var(--r-card);
  padding: 22px;
  text-align: center;
  box-shadow: var(--shadow);
}
.modal-icon {
  color: var(--danger);
  display: flex;
  justify-content: center;
  margin-bottom: 10px;
}
.modal-text {
  white-space: pre-line;
  font-size: 14px;
  margin: 0 0 20px;
}
.modal-actions {
  display: flex;
  gap: 8px;
  justify-content: center;
}
.modal-actions .btn {
  min-width: 108px;
}
</style>
