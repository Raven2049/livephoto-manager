<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary, type ImageItem } from "./stores/library";
import { useZoom } from "./composables/useZoom";
import PhotoGrid from "./components/PhotoGrid.vue";

const lib = useLibrary();
const zoom = useZoom(5);
const grid = ref<InstanceType<typeof PhotoGrid> | null>(null);
const main = ref<HTMLElement | null>(null);

async function pickLibrary() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === "string") await lib.openLibrary(dir);
}

const imageItems = computed<ImageItem[]>(() =>
  lib.assets
    .filter((a) => !a.missing && a.still_path)
    .map((a) => ({
      path: a.still_path as string,
      name: a.base_name,
      size: 0,
    })),
);

function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return; // 只有 Ctrl+滚轮才缩放
  e.preventDefault();
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
      <span v-if="lib.busy">处理中…</span>
      <span v-else-if="lib.error" class="err">{{ lib.error }}</span>
      <span v-else-if="lib.stats">
        共 {{ lib.stats.total }} · 实况 {{ lib.stats.live }} · 照片 {{ lib.stats.photo }} ·
        视频 {{ lib.stats.video }} · 缺失 {{ lib.stats.missing }} · {{ zoom.columns.value }} 列
      </span>
      <span v-else>未打开库</span>
    </header>
    <main ref="main">
      <PhotoGrid ref="grid" :items="imageItems" :columns="zoom.columns.value" :gap="8" />
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
}
</style>
