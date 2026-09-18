import { ref, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { lpmUrl } from "../lib/lpm";
import type { AssetRow } from "../stores/library";

/** 悬停停留多久才触发预览片生成（设计 §7.3）。 */
const HOVER_DELAY = 350;
/** 滚动/缩放结束后的抑制窗口（设计 §7.3）。 */
const SUPPRESS_AFTER = 300;
/** 同时最多几个预览片生成任务在飞（设计 §7.3）。 */
const MAX_IN_FLIGHT = 2;

/**
 * 悬停预览：全局复用**唯一一个** `<video>` 元素（设计 §8.1 决定 1）。
 *
 * 一万个瓦片各挂一个 `<video>` 会直接拖垮 WebView2，所以这里只操作调用方
 * 传入的那一个 video 引用，由调用方把它定位到当前悬停的瓦片上。
 *
 * @param video 唯一那个 `<video>` 元素的模板引用（由调用方持有并绑定）
 */
export function usePreview(video: Ref<HTMLVideoElement | null>) {
  /** 正在生成/缓冲，尚未出画面。 */
  const loading = ref(false);
  /** 已有画面，`<video>` 应可见。 */
  const visible = ref(false);

  let hoverTimer: number | undefined;
  let suppressUntil = 0;
  let inFlight = 0;
  let currentAssetId: number | null = null;
  /** 每次 hide/新的 play 都自增，用于丢弃过期的异步结果。 */
  let token = 0;

  function clearHoverTimer() {
    if (hoverTimer !== undefined) {
      clearTimeout(hoverTimer);
      hoverTimer = undefined;
    }
  }

  function resetVideo() {
    const v = video.value;
    if (!v) return;
    v.pause();
    v.removeAttribute("src");
    v.load();
  }

  /** 收起当前预览。已收起时不做任何 DOM 操作（滚动会高频调用）。 */
  function hide() {
    clearHoverTimer();
    token++;
    if (!visible.value && !loading.value && currentAssetId === null) return;
    visible.value = false;
    loading.value = false;
    currentAssetId = null;
    resetVideo();
  }

  /** 滚动/缩放时调用：抑制触发，并收起当前预览。 */
  function suppress() {
    suppressUntil = Date.now() + SUPPRESS_AFTER;
    hide();
  }

  /**
   * 悬停进入某个瓦片。
   * `asset` 为 null、或该条目没有视频（普通照片）时不播放。
   */
  function enter(asset: AssetRow | null) {
    clearHoverTimer();
    if (!asset || !asset.movie_path) {
      hide();
      return;
    }
    // 换到另一个瓦片时，先收起上一个，避免旧画面停在错误的格子上。
    if (currentAssetId !== asset.id) hide();
    if (Date.now() < suppressUntil) return;
    hoverTimer = window.setTimeout(() => void play(asset), HOVER_DELAY);
  }

  async function play(asset: AssetRow) {
    if (visible.value && currentAssetId === asset.id) return;
    const v = video.value;
    if (!v) return;
    // 超限直接放弃本次；悬停预览允许丢，不该排队堆积。
    if (inFlight >= MAX_IN_FLIGHT) return;

    inFlight++;
    const my = ++token;
    loading.value = true;
    try {
      const path = await invoke<string>("ensure_preview", { assetId: asset.id });
      if (my !== token) return;

      currentAssetId = asset.id;
      v.src = lpmUrl(path);
      v.load();
      await onceCanPlay(v);
      if (my !== token) return;

      visible.value = true;
      loading.value = false;
      await v.play().catch(() => {});
    } catch {
      if (my === token) {
        visible.value = false;
        loading.value = false;
        currentAssetId = null;
      }
    } finally {
      inFlight--;
    }
  }

  /** 等首帧可播（或出错），避免 `<video>` 先显示一块黑底。 */
  function onceCanPlay(v: HTMLVideoElement): Promise<void> {
    return new Promise((resolve) => {
      const done = () => {
        v.removeEventListener("canplay", done);
        v.removeEventListener("error", done);
        resolve();
      };
      v.addEventListener("canplay", done);
      v.addEventListener("error", done);
    });
  }

  return { loading, visible, enter, suppress, hide };
}
