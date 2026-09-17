import { defineStore } from "pinia";
import { shallowRef, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface ImageItem {
  path: string;
  name: string;
  size: number;
}

export const useLibrary = defineStore("library", () => {
  // 设计 §8.3：上万条对象绝不能被 Vue 深层代理，必须 shallowRef。
  const items = shallowRef<ImageItem[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load(dir: string) {
    loading.value = true;
    error.value = null;
    try {
      items.value = await invoke<ImageItem[]>("scan_dir", { path: dir });
    } catch (e) {
      error.value = String(e);
      items.value = [];
    } finally {
      loading.value = false;
    }
  }

  return { items, loading, error, load };
});
