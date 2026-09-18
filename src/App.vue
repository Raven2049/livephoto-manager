<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary, type AssetRow } from "./stores/library";
import { useImport } from "./stores/import";
import { usePreview } from "./composables/usePreview";
import { useZoom } from "./composables/useZoom";
import { groupAssets, granularityForColumns, type Granularity } from "./lib/timeline";
import PhotoGrid from "./components/PhotoGrid.vue";
import FilterBar from "./components/FilterBar.vue";
import AppIcon from "./components/AppIcon.vue";

const lib = useLibrary();
const imp = useImport();
const zoom = useZoom(5);
const grid = ref<InstanceType<typeof PhotoGrid> | null>(null);
const main = ref<HTMLElement | null>(null);
const assumeCloud = ref(false);

/* ---------- Toast ---------- */
type Toast = { id: number; text: string; kind: "info" | "error" };
const toasts = ref<Toast[]>([]);
let toastSeq = 0;
function toast(text: string, kind: "info" | "error" = "info") {
  const id = ++toastSeq;
  toasts.value = [...toasts.value, { id, text, kind }];
  window.setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }, 4000);
}
function toastError() {
  if (lib.error) {
    toast(lib.error, "error");
    lib.error = null;
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
  if (imp.error) toast(imp.error, "error");
  else if (imp.progress) {
    const { done, total, failed } = imp.progress;
    toast(`导入完成 ${done}/${total}${failed ? ` · 失败 ${failed}` : ""}`, failed ? "error" : "info");
  }
}

async function doRescan() {
  await lib.rescan();
  if (lib.error) toastError();
  else toast("索引已重建");
}

async function doThumbs() {
  await lib.generateThumbs();
  if (lib.error) toastError();
  else if (lib.thumbs) {
    toast(`缩略图 ${lib.thumbs.done}/${lib.thumbs.total}（失败 ${lib.thumbs.failed}）`);
  }
}

async function doClassify() {
  await lib.classify();
  if (lib.error) toastError();
  else toast("标识校验完成");
}

async function exportDiag() {
  try {
    const path = await lib.exportDiagnostics();
    toast(`诊断报告已生成：${path}`);
  } catch (e) {
    toast(String(e), "error");
  }
}

async function exportSelected() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir !== "string") return;
  const n = lib.selected.size;
  await lib.exportSelected(dir);
  if (lib.error) toastError();
  else toast(`已导出 ${n} 个条目到 ${dir}`);
}

async function deleteSelected() {
  const n = lib.selected.size;
  if (!n) return;
  const ok = await askConfirm(
    `将删除 ${n} 个条目并移入回收站（实况条目会同时删除静态图与视频）。\n回收站容量不足时，大文件可能被永久删除。`,
  );
  if (!ok) return;
  await lib.deleteSelected();
  if (lib.error) toastError();
  else toast(`已删除 ${n} 个条目`);
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

function onNearEnd() {
  void lib.loadMore();
}
function onToggleSelect(id: number) {
  lib.toggleSelect(id);
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
onMounted(() => {
  main.value?.addEventListener("wheel", onWheel, { passive: false });
});
onBeforeUnmount(() => {
  main.value?.removeEventListener("wheel", onWheel);
});

/* ---------- 进度与统计 ---------- */
const importPct = computed(() => {
  const p = imp.progress;
  return p && p.total ? Math.round((p.done / p.total) * 100) : 0;
});
const importMB = computed(() => ((imp.progress?.bytes_done ?? 0) / 1048576).toFixed(1));
const thumbPct = computed(() => {
  const t = lib.thumbs;
  return t && t.total ? Math.round((t.done / t.total) * 100) : 0;
});

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
  <div class="app">
    <!-- ================= 侧栏 ================= -->
    <aside class="sidebar">
      <div class="brand">
        <div class="logo">L</div>
        <div>
          <h1>LivePorter</h1>
          <div class="sub">iPhone 实况照片搬运</div>
        </div>
      </div>

      <div class="sec">
        <div class="sec-title">资料库</div>
        <div class="card">
          <div class="libpath">
            <AppIcon name="folder" :size="14" />
            <span>{{ lib.root || "未打开资料库" }}</span>
          </div>
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
          <AppIcon name="iphone" />从 iPhone 导入
        </button>
        <label class="switch" :class="{ on: assumeCloud }" @click.prevent="assumeCloud = !assumeCloud">
          <span class="track"><i></i></span>原件可能不在手机
        </label>

        <template v-if="imp.running">
          <div class="progress"><i :style="{ width: importPct + '%' }"></i></div>
          <div class="ptext">
            <span>导入中 {{ imp.progress?.done ?? 0 }}/{{ imp.progress?.total ?? "?" }}</span>
            <span>{{ importMB }} MB</span>
          </div>
          <button class="btn block" style="margin-top: 8px" @click="imp.stop">
            <AppIcon name="stop" />停止
          </button>
        </template>
        <template v-else-if="lib.busy">
          <div class="progress"><i style="width: 40%"></i></div>
          <div class="ptext"><span>处理中…</span></div>
        </template>
        <template v-else-if="lib.thumbs">
          <div class="progress"><i :style="{ width: thumbPct + '%' }"></i></div>
          <div class="ptext">
            <span>缩略图 {{ lib.thumbs.done }}/{{ lib.thumbs.total }}</span>
            <span>失败 {{ lib.thumbs.failed }}</span>
          </div>
        </template>
      </div>

      <div class="sec">
        <div class="sec-title">维护</div>
        <button class="action" :disabled="!lib.root || lib.busy" @click="doRescan">
          <AppIcon name="refresh" />重建索引
        </button>
        <button class="action" :disabled="!lib.root || lib.busy" @click="doThumbs">
          <AppIcon name="image" />生成缩略图
        </button>
        <button class="action" :disabled="!lib.root || lib.busy" @click="doClassify">
          <AppIcon name="shield" />校验标识
        </button>
        <button class="action" :disabled="!lib.root || lib.busy" @click="exportDiag">
          <AppIcon name="doc" />导出诊断报告
        </button>
      </div>

      <div class="spacer"></div>
      <div class="sidefoot">v1.0.0 · GPL-3.0 · ffmpeg 9.0.1</div>
    </aside>

    <!-- ================= 主区 ================= -->
    <section class="main">
      <div class="toolbar">
        <span class="title">资料库</span>
        <FilterBar />
        <div class="status">
          已加载 {{ lib.assets.length.toLocaleString() }}/{{ lib.total.toLocaleString() }} ·
          {{ zoom.columns.value }} 列
        </div>
      </div>

      <div class="stage" ref="main">
        <PhotoGrid
          ref="grid"
          :groups="groups"
          :columns="zoom.columns.value"
          :gap="8"
          :granularity="gran"
          :selected="lib.selected"
          :has-more="lib.assets.length < lib.total"
          @hover="onHover"
          @hover-out="onHoverOut"
          @suppress="onSuppress"
          @toggle-select="onToggleSelect"
          @near-end="onNearEnd"
        />

        <!-- 空状态 -->
        <div v-if="!lib.root" class="empty">
          <div class="empty-card">
            <div class="empty-logo">L</div>
            <h2>打开资料库开始</h2>
            <p class="muted">选择一个硬盘目录作为资料库，导入的实况照片都会放在里面。</p>
            <button class="btn prominent" @click="pickLibrary">打开资料库…</button>
          </div>
        </div>
        <div v-else-if="noResults" class="empty">
          <div class="empty-card">
            <h2>{{ filtersActive ? "没有匹配项" : "资料库还是空的" }}</h2>
            <p class="muted">
              {{
                filtersActive
                  ? "换个关键词或清空筛选试试。"
                  : "插上 iPhone，点侧栏的「从 iPhone 导入」。"
              }}
            </p>
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
      <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">
        <AppIcon :name="t.kind === 'error' ? 'warn' : 'check'" :size="15" />
        <span>{{ t.text }}</span>
      </div>
    </div>

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
.logo,
.empty-logo {
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
.sec {
  padding: 4px 12px 12px;
}
.sec-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--label-3);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin: 8px 6px 6px;
}
.libpath {
  display: flex;
  gap: 7px;
  align-items: flex-start;
  font-size: 12px;
  color: var(--label-2);
  word-break: break-all;
  line-height: 1.35;
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
  color: var(--label-3);
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
.status {
  margin-left: auto;
  color: var(--label-2);
  font-size: 12px;
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
.empty-card .empty-logo {
  margin: 0 auto 12px;
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
  border-radius: var(--r-tile);
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
