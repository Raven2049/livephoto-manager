import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ImportProgress {
  total: number;
  done: number;
  failed: number;
  current: string;
  bytes_done: number;
  bytes_total: number;
  cancelled: boolean;
}

export const useImport = defineStore("import", () => {
  const running = ref(false);
  const progress = ref<ImportProgress | null>(null);
  const error = ref<string | null>(null);

  async function start() {
    running.value = true;
    error.value = null;
    // 先订阅进度事件，再发起命令。
    const unlisten = await listen<ImportProgress>("import://progress", (e) => {
      progress.value = e.payload;
    });
    try {
      progress.value = await invoke<ImportProgress>("import_from_device");
    } catch (e) {
      error.value = String(e);
    } finally {
      unlisten();
      running.value = false;
    }
  }

  /** 请求停止当前导入（后端在文件之间检查）。 */
  async function stop() {
    await invoke("cancel_import");
  }

  return { running, progress, error, start, stop };
});
