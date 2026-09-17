<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useLibrary } from "./stores/library";
import { useZoom } from "./composables/useZoom";
import PhotoGrid from "./components/PhotoGrid.vue";

const lib = useLibrary();
const zoom = useZoom(5);
const grid = ref<InstanceType<typeof PhotoGrid> | null>(null);
const main = ref<HTMLElement | null>(null);

async function pick() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === "string") await lib.load(dir);
}

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
      <button @click="pick">选择文件夹</button>
      <span v-if="lib.loading">扫描中…</span>
      <span v-else-if="lib.error">{{ lib.error }}</span>
      <span v-else>{{ lib.items.length }} 张 · {{ zoom.columns.value }} 列</span>
    </header>
    <main ref="main">
      <PhotoGrid ref="grid" :items="lib.items" :columns="zoom.columns.value" :gap="8" />
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
main {
  position: relative;
  flex: 1;
  min-height: 0;
}
</style>
