import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * 把绝对路径转成 lpm:// 协议 URL。
 *
 * Windows 上实际形态是 `http://lpm.localhost/<encodeURIComponent(path)>`，
 * 但必须走 convertFileSrc，不能手拼（跨平台形态不同）。
 */
export function lpmUrl(absPath: string): string {
  return convertFileSrc(absPath, "lpm");
}
