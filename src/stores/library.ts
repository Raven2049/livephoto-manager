import { defineStore } from "pinia";
import { shallowRef, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface AssetRow {
  id: number;
  kind: number;
  integrity: number;
  base_name: string;
  still_path: string | null;
  movie_path: string | null;
  thumb_path: string | null;
  missing: boolean;
}

export interface LibraryStats {
  total: number;
  live: number;
  photo: number;
  video: number;
  missing: number;
  by_integrity: [number, number][];
}

/** 供 PhotoGrid 显示的最小结构。 */
export interface ImageItem {
  path: string;
  name: string;
  size: number;
}

export const useLibrary = defineStore("library", () => {
  const root = ref<string | null>(null);
  // 设计 §8.3：上万条对象绝不能被 Vue 深层代理，必须 shallowRef。
  const assets = shallowRef<AssetRow[]>([]);
  const stats = ref<LibraryStats | null>(null);
  const busy = ref(false);
  const error = ref<string | null>(null);

  async function openLibrary(path: string) {
    busy.value = true;
    error.value = null;
    try {
      await invoke("open_library", { path });
      root.value = path;
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      busy.value = false;
    }
  }

  async function rescan() {
    busy.value = true;
    error.value = null;
    try {
      await invoke("scan_library");
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      busy.value = false;
    }
  }

  const thumbs = ref<{ total: number; done: number; failed: number } | null>(null);

  async function generateThumbs() {
    busy.value = true;
    error.value = null;
    try {
      thumbs.value = await invoke("generate_thumbs");
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      busy.value = false;
    }
  }

  async function classify() {
    busy.value = true;
    error.value = null;
    try {
      await invoke("classify_library");
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      busy.value = false;
    }
  }

  async function exportDiagnostics(): Promise<string> {
    return await invoke<string>("export_diagnostics");
  }

  async function refresh() {
    stats.value = await invoke<LibraryStats>("library_stats");
    // 本计划先一次性取前 5000 条；分页 + 虚拟滚动的联动属后续计划。
    assets.value = await invoke<AssetRow[]>("list_assets", {
      offset: 0,
      limit: 5000,
    });
  }

  return {
    root,
    assets,
    stats,
    busy,
    error,
    thumbs,
    openLibrary,
    rescan,
    generateThumbs,
    classify,
    exportDiagnostics,
    refresh,
  };
});
