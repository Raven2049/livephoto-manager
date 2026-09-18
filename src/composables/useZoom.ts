import { ref } from "vue";

// 离散档位：列数越多 = 瓦片越小 = 越"缩小"。
// 1 列是「单列大图流」（瀑布流式浏览），3 列是普通网格的最放大档。
const LEVELS = [1, 3, 5, 7, 10, 14] as const;

/**
 * 最大放大档位（最小的列数）受 DPR 限制：
 * 瓦片 CSS 边长 * dpr 不应超过缩略图边长 THUMB_PX（设计 §7.4，缩略图 512px）。
 * 目前没有缩略图、直接吃原图，故上限按 512 仍保守成立。
 */
export const THUMB_PX = 512;

export function maxColumnsForDpr(viewportCssWidth: number, dpr: number): number {
  // 允许的最大瓦片边长（CSS 像素）
  const maxTileCss = THUMB_PX / Math.max(1, dpr);
  // 最少需要多少列才能让瓦片不超过该边长
  return Math.ceil(viewportCssWidth / maxTileCss);
}

export function useZoom(initial: number) {
  const columns = ref(initial);

  function allowedLevels(viewportCssWidth: number, dpr: number): number[] {
    const minCols = maxColumnsForDpr(viewportCssWidth, dpr);
    const levels: number[] = LEVELS.filter((c) => c >= minCols);
    // 最大放大档（最小列数）永远可用：它是单列浏览档，用于瀑布流式大图阅读。
    // 该档可能超过缩略图 512px 的清晰上限（设计 §7.4 的取舍），属有意为之。
    if (levels[0] !== LEVELS[0]) levels.unshift(LEVELS[0]);
    return levels;
  }

  /** 返回下一次缩放后的列数；到达边界时返回原值。delta<0 表示放大（列数变少）。 */
  function next(delta: number, viewportCssWidth: number, dpr: number): number {
    const levels = allowedLevels(viewportCssWidth, dpr);
    if (levels.length === 0) return LEVELS[LEVELS.length - 1];
    const cur = columns.value;
    if (delta < 0) {
      // 放大：选比当前小的最大档
      const smaller = [...levels].reverse().filter((c) => c < cur);
      return smaller[0] ?? levels[0];
    } else {
      // 缩小：选比当前大的最小档
      const bigger = levels.filter((c) => c > cur);
      return bigger[0] ?? levels[levels.length - 1];
    }
  }

  return { columns, next, allowedLevels };
}
