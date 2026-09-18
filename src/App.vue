<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary, type AssetRow, type ImageItem } from "./stores/library";
import { useImport } from "./stores/import";
import { usePreview } from "./composables/usePreview";
import { useZoom } from "./composables/useZoom";
import PhotoGrid from "./components/PhotoGrid.vue";

const lib = useLibrary();
const imp = useImport();
const zoom = useZoom(5);
const grid = ref<InstanceType<typeof PhotoGrid> | null>(null);
const main = ref<HTMLElement | null>(null);
const assumeCloud = ref(false);

// 全局唯一一个 <video>（设计 §8.1 决定 1），由 hover 事件定位到目标瓦片。
const videoEl = ref<HTMLVideoElement | null>(null);
const pv = usePreview(videoEl);
/** 当前悬停瓦片相对 main 的矩形；null 表示没有预览。 */
const hoverRect = ref<{ x: number; y: number; w: number; h: number } | null>(null);

async function pickLibrary() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === "string") await lib.openLibrary(dir);
}

async function importFromDevice() {
  await imp.start(assumeCloud.value);
  await lib.refresh();
}

// 可显示的条目。gridAssets 与 imageItems 下标一一对应，PhotoGrid 靠下标取回
// 原始条目（id / movie_path）用于悬停预览。
const gridAssets = computed<AssetRow[]>(() =>
  lib.assets.filter((a) => !a.missing && (a.thumb_path || a.still_path)),
);

const imageItems = computed<ImageItem[]>(() =>
  gridAssets.value.map((a) => ({
    // 优先用缩略图（HEIC 原图 WebView2 解不了）。
    path: (a.thumb_path || a.still_path) as string,
    name: a.base_name,
    size: 0,
  })),
);

// 仅在存在异常项（1 不一致 / 2 残缺 / 5 疑似副本）时提示。
const integrityHint = computed(() => {
  const s = lib.stats;
  if (!s) return "";
  const labels: Record<number, string> = { 1: "不一致", 2: "残缺", 5: "疑似副本" };
  const abnormal = s.by_integrity.filter(([k]) => k in labels);
  if (!abnormal.length) return "";
  return " · 异常 " + abnormal.map(([k, n]) => `${labels[k]}${n}`).join(" ");
});

const slotStyle = computed(() => {
  const r = hoverRect.value;
  if (!r) return {};
  return {
    left: `${r.x}px`,
    top: `${r.y}px`,
    width: `${r.w}px`,
    height: `${r.h}px`,
  };
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

/** 滚动或缩放：收起预览并抑制新的触发（设计 §7.3）。 */
function onSuppress() {
  pv.suppress();
  hoverRect.value = null;
}

function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return; // 只有 Ctrl+滚轮才缩放
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
</script>

<template>
  <div class="app">
    <header>
      <button @click="pickLibrary">打开库</button>
      <button :disabled="!lib.root || lib.busy" @click="lib.rescan">重建索引</button>
      <button :disabled="!lib.root || lib.busy" @click="lib.generateThumbs">生成缩略图</button>
      <button :disabled="!lib.root || lib.busy" @click="lib.classify">校验标识</button>
      <button :disabled="!lib.root || imp.running" @click="importFromDevice">
        从 iPhone 导入
      </button>
      <label class="cloud">
        <input type="checkbox" v-model="assumeCloud" />
        原件可能不在手机
      </label>
      <button v-if="imp.running" @click="imp.stop">停止</button>
      <span v-if="imp.running" class="prog">
        导入中 {{ imp.progress?.done ?? 0 }}/{{ imp.progress?.total ?? "?" }}
        · {{ imp.progress?.current }} · {{ imp.progress?.bytes_done ?? 0 }} 字节
        <template v-if="imp.progress?.failed"> · 失败 {{ imp.progress.failed }}</template>
      </span>
      <span v-else-if="lib.busy">处理中…</span>
      <span v-else-if="lib.thumbs">
        缩略图 {{ lib.thumbs.done }}/{{ lib.thumbs.total }}（失败 {{ lib.thumbs.failed }}）
      </span>
      <span v-else-if="lib.error" class="err">{{ lib.error }}</span>
      <span v-else-if="imp.error" class="err">{{ imp.error }}</span>
      <span v-else-if="lib.stats">
        共 {{ lib.stats.total }} · 实况 {{ lib.stats.live }} · 照片 {{ lib.stats.photo }} ·
        视频 {{ lib.stats.video }} · 缺失 {{ lib.stats.missing }} · {{ zoom.columns.value }} 列{{
          integrityHint
        }}
      </span>
      <span v-else>未打开库</span>
    </header>
    <main ref="main">
      <PhotoGrid
        ref="grid"
        :items="imageItems"
        :assets="gridAssets"
        :columns="zoom.columns.value"
        :gap="8"
        @hover="onHover"
        @hover-out="onHoverOut"
        @suppress="onSuppress"
      />
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
    </main>
  </div>
</template>

<style>
html,
body,
#app {
  height: 100%;
  margin: 0;
}
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #111;
  color: #eee;
}
header {
  padding: 8px;
  display: flex;
  gap: 12px;
  align-items: center;
}
.err {
  color: #f88;
}
main {
  position: relative;
  flex: 1;
  min-height: 0;
  /* 瓦片滚出视口时，悬停预览也要被裁掉 */
  overflow: hidden;
}

/* 悬停预览：位置和尺寸都跟随目标瓦片 */
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
}
.preview.show {
  opacity: 1;
}
/* 生成/缓冲期间的一个淡进度点，避免出现空白格 */
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
</style>
