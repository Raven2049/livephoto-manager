import { defineStore } from "pinia";
import { shallowRef, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface AssetRow {
  id: number;
  kind: number;
  integrity: number;
  taken_at: number;
  base_name: string;
  still_path: string | null;
  movie_path: string | null;
  thumb_path: string | null;
  missing: boolean;
  /** 缩略图尺寸（瀑布流用；无缩略图时为 null）。 */
  thumb_w: number | null;
  thumb_h: number | null;
}

export interface LibraryStats {
  total: number;
  live: number;
  photo: number;
  video: number;
  missing: number;
  missing_thumbs: number;
  by_integrity: [number, number][];
}

export interface ScanProgress {
  devices: number;
  files: number;
  assets: number;
}

export interface RecentLibrary {
  path: string;
  name: string;
  last_opened: number;
  exists: boolean;
}

/** 供 PhotoGrid 显示的最小结构。 */
export interface ImageItem {
  path: string;
  name: string;
  size: number;
}

/** 前端筛选状态（日期是 "YYYY-MM-DD"，提交后端前换算成 epoch 秒）。 */
export interface AssetFilter {
  text: string;
  kind: number | null;
  integrity: number[];
  from: string | null;
  to: string | null;
}

export type GranularityOverride = "auto" | "day" | "month" | "year";

export interface ExportProgress {
  total: number;
  done: number;
  failed: number;
  bytes_done: number;
  bytes_total: number;
}

export interface ExportSummary {
  total: number;
  done: number;
  failed: number;
  errors: string[];
}

export interface DeleteProgress {
  total: number;
  done: number;
  failed: number;
}

export interface DeleteSummary {
  total: number;
  deleted: number;
  failed: number;
  errors: string[];
}

export const PAGE_SIZE = 1000;

function backendFilter(f: AssetFilter) {
  const dayStart = (s: string | null) =>
    s ? Math.floor(new Date(`${s}T00:00:00`).getTime() / 1000) : null;
  const dayEnd = (s: string | null) =>
    s ? Math.floor(new Date(`${s}T23:59:59`).getTime() / 1000) : null;
  return {
    text: f.text.trim() || null,
    kind: f.kind,
    integrity: f.integrity.length ? f.integrity : null,
    from: dayStart(f.from),
    to: dayEnd(f.to),
  };
}

export const useLibrary = defineStore("library", () => {
  const root = ref<string | null>(null);
  // 设计 §8.3：上万条对象绝不能被 Vue 深层代理，必须 shallowRef。
  const assets = shallowRef<AssetRow[]>([]);
  const stats = ref<LibraryStats | null>(null);
  const busy = ref(false);
  const error = ref<string | null>(null);

  // 过滤 + 分页 + 选中（计划 8）。
  const filter = reactive<AssetFilter>({
    text: "",
    kind: null,
    integrity: [],
    from: null,
    to: null,
  });
  const total = ref(0);
  const loading = ref(false);
  const selected = ref<Set<number>>(new Set());
  const granOverride = ref<GranularityOverride>("auto");

  function setSelected(next: Set<number>) {
    // 整体替换，保证 Vue 能追踪 Set 的变化。
    selected.value = next;
  }

  async function openLibrary(path: string) {
    busy.value = true;
    error.value = null;
    try {
      await invoke("open_library", { path });
      root.value = path;
      await refresh();
      // 自动维护：索引为空但有文件 → 重建索引；有缺缩略图 → 自动补。
      if ((stats.value?.total ?? 0) === 0) await rescan();
      if ((stats.value?.missing_thumbs ?? 0) > 0) await generateThumbs();
      await loadRecents();
    } catch (e) {
      error.value = String(e);
    } finally {
      busy.value = false;
    }
  }

  /* ---------- 最近打开的资料库 ---------- */
  const recents = ref<RecentLibrary[]>([]);

  async function loadRecents() {
    try {
      recents.value = await invoke<RecentLibrary[]>("recent_libraries");
    } catch {
      /* 读取失败就当作没有缓存 */
    }
  }

  async function forgetRecent(path: string) {
    try {
      recents.value = await invoke<RecentLibrary[]>("forget_library", { path });
    } catch (e) {
      error.value = String(e);
    }
  }

  /** 重建索引期间为真：界面显示细进度条（非阻塞）。 */
  const scanning = ref(false);
  const scanProgress = ref<ScanProgress | null>(null);

  async function rescan() {
    busy.value = true;
    scanning.value = true;
    scanProgress.value = null;
    error.value = null;
    const un = await listen<ScanProgress>("scan://progress", (e) => {
      scanProgress.value = e.payload;
    });
    try {
      await invoke("scan_library");
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      un();
      busy.value = false;
      scanning.value = false;
      scanProgress.value = null;
    }
  }

  async function cancelScan() {
    await invoke("cancel_scan");
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

  const classifyProgress = ref<{ total: number; done: number } | null>(null);

  /** 返回是否被取消。 */
  async function classify(): Promise<boolean> {
    busy.value = true;
    error.value = null;
    classifyProgress.value = null;
    const un = await listen<{ total: number; done: number }>(
      "classify://progress",
      (e) => {
        classifyProgress.value = e.payload;
      },
    );
    try {
      const r = await invoke<{ total: number; changed: number; cancelled: boolean }>(
        "classify_library",
      );
      await refresh();
      return r.cancelled;
    } catch (e) {
      error.value = String(e);
      return false;
    } finally {
      un();
      busy.value = false;
      classifyProgress.value = null;
    }
  }

  async function exportDiagnostics(): Promise<string> {
    return await invoke<string>("export_diagnostics");
  }

  /** 重置分页并重新加载第一页；筛选变化与库变更后调用。 */
  async function reload() {
    loading.value = true;
    error.value = null;
    try {
      const f = backendFilter(filter);
      const [rows, n] = await Promise.all([
        invoke<AssetRow[]>("list_assets", { offset: 0, limit: PAGE_SIZE, filter: f }),
        invoke<number>("count_assets", { filter: f }),
      ]);
      assets.value = rows;
      total.value = n;
      setSelected(new Set());
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  /** 追加下一页（滚动接近底部时由网格触发）。 */
  async function loadMore() {
    if (loading.value || assets.value.length >= total.value) return;
    loading.value = true;
    try {
      const f = backendFilter(filter);
      const next = await invoke<AssetRow[]>("list_assets", {
        offset: assets.value.length,
        limit: PAGE_SIZE,
        filter: f,
      });
      // shallowRef 必须整体替换。
      assets.value = [...assets.value, ...next];
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function refreshStats() {
    stats.value = await invoke<LibraryStats>("library_stats");
  }

  function toggleSelect(id: number) {
    const next = new Set(selected.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  }

  /** 批量设置选中态（用于拖拽涂抹选择）。 */
  function setSelection(ids: number[], value: boolean) {
    if (!ids.length) return;
    const next = new Set(selected.value);
    for (const id of ids) {
      if (value) next.add(id);
      else next.delete(id);
    }
    setSelected(next);
  }

  /** 只选中一项（右键菜单用）。 */
  function selectOnly(id: number) {
    setSelected(new Set([id]));
  }

  /** 清空全部筛选条件并重新加载。 */
  function clearFilters() {
    filter.text = "";
    filter.kind = null;
    filter.integrity = [];
    filter.from = null;
    filter.to = null;
    void reload();
  }

  /** 选中当前筛选条件下的整库（后端只回 id）。 */
  async function selectAll() {
    const ids = await invoke<number[]>("list_asset_ids", {
      filter: backendFilter(filter),
    });
    setSelected(new Set(ids));
  }

  function clearSelection() {
    setSelected(new Set());
  }

  const exporting = ref(false);
  const exportProgress = ref<ExportProgress | null>(null);

  /** 把选中条目原样拷贝到 `dest` 目录。 */
  async function exportSelected(dest: string) {
    const ids = [...selected.value];
    if (!ids.length) return;
    exporting.value = true;
    error.value = null;
    const un = await listen<ExportProgress>("export://progress", (e) => {
      exportProgress.value = e.payload;
    });
    try {
      const r = await invoke<ExportSummary>("export_assets", { ids, dest });
      if (r.failed) error.value = `导出完成，失败 ${r.failed} 项`;
    } catch (e) {
      error.value = String(e);
    } finally {
      un();
      exporting.value = false;
      exportProgress.value = null;
    }
  }

  const deleting = ref(false);
  const deleteProgress = ref<DeleteProgress | null>(null);

  /** 把选中条目移入回收站并清理索引/缓存。 */
  async function deleteSelected() {
    const ids = [...selected.value];
    if (!ids.length) return;
    deleting.value = true;
    error.value = null;
    const un = await listen<DeleteProgress>("delete://progress", (e) => {
      deleteProgress.value = e.payload;
    });
    try {
      const r = await invoke<DeleteSummary>("delete_assets", { ids });
      if (r.failed) error.value = `删除完成，失败 ${r.failed} 项`;
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      un();
      deleting.value = false;
      deleteProgress.value = null;
    }
  }

  /** 兼容既有调用：刷新统计 + 重置分页。 */
  async function refresh() {
    await refreshStats();
    await reload();
  }

  return {
    root,
    recents,
    loadRecents,
    forgetRecent,
    assets,
    stats,
    busy,
    scanning,
    scanProgress,
    cancelScan,
    classifyProgress,
    error,
    thumbs,
    filter,
    total,
    loading,
    selected,
    granOverride,
    openLibrary,
    rescan,
    generateThumbs,
    classify,
    exportDiagnostics,
    reload,
    loadMore,
    refreshStats,
    refresh,
    toggleSelect,
    setSelection,
    selectOnly,
    clearFilters,
    selectAll,
    clearSelection,
    exporting,
    exportProgress,
    exportSelected,
    deleting,
    deleteProgress,
    deleteSelected,
  };
});
