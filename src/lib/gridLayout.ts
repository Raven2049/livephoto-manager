import type { AssetRow } from "../stores/library";
import type { AssetGroup, Granularity } from "./timeline";

export interface TileCell {
  asset: AssetRow;
  index: number;
  x: number;
  size: number;
}

export type GridRow =
  | { type: "header"; key: string; label: string; count: number; y: number; h: number }
  | { type: "tiles"; key: string; y: number; h: number; cells: TileCell[] }
  | { type: "footer"; key: "footer"; y: number; h: number };

export interface Layout {
  rows: GridRow[];
  totalH: number;
  /** 全局序号 → 该瓦片左上角的 y（缩放锚点用）。 */
  indexToY: (index: number) => number;
}

export const FOOTER_H = 48;

/** 各粒度的标题行高度：年最粗最高，天最细最矮。 */
export function headerHeight(gran: Granularity): number {
  return gran === "year" ? 64 : gran === "month" ? 52 : 44;
}

/**
 * 把时间线分组铺成带 y 偏移的行序列。
 * 每组 = 一个 header 行 + ceil(n/columns) 个瓦片行（正方形，边长 tileW）。
 */
export function buildLayout(
  groups: AssetGroup[],
  columns: number,
  tileW: number,
  gap: number,
  hasMore: boolean,
  headerH: number,
): Layout {
  const cols = Math.max(1, columns);
  const size = tileW < 1 ? 1 : tileW;
  const stride = size + gap;
  const rows: GridRow[] = [];
  const groupStartY: number[] = [];
  const groupStartIndex: number[] = [];
  let y = gap;
  let index = 0;

  for (const g of groups) {
    groupStartY.push(y);
    groupStartIndex.push(index);
    rows.push({
      type: "header",
      key: g.key,
      label: g.label,
      count: g.assets.length,
      y,
      h: headerH,
    });
    y += headerH;

    const nRows = Math.ceil(g.assets.length / cols);
    for (let r = 0; r < nRows; r++) {
      const cells: TileCell[] = [];
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c;
        if (i >= g.assets.length) break;
        cells.push({
          asset: g.assets[i],
          index: index + i,
          x: gap + c * stride,
          size,
        });
      }
      rows.push({ type: "tiles", key: `${g.key}#${r}`, y, h: stride, cells });
      y += stride;
    }
    index += g.assets.length;
  }

  if (hasMore) {
    rows.push({ type: "footer", key: "footer", y, h: FOOTER_H });
    y += FOOTER_H;
  }

  return {
    rows,
    totalH: y,
    indexToY(i: number) {
      if (i < 0 || groups.length === 0) return 0;
      let lo = 0;
      let hi = groupStartIndex.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (groupStartIndex[mid] <= i) lo = mid;
        else hi = mid - 1;
      }
      const within = i - groupStartIndex[lo];
      return groupStartY[lo] + headerH + Math.floor(within / cols) * stride;
    },
  };
}
