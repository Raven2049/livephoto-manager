import type { AssetRow } from "../stores/library";
import type { AssetGroup, Granularity } from "./timeline";

export interface TileCell {
  asset: AssetRow;
  index: number;
  x: number;
  y: number;
  w: number;
  h: number;
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
  /** 全局序号 → 该瓦片高度（缩放锚点用）。 */
  indexHeight: (index: number) => number;
}

export const FOOTER_H = 48;
/** 瀑布流里单张的相对高宽比上下限，避免极端细长/压扁。 */
const MIN_RATIO = 0.5;
const MAX_RATIO = 2.4;

/** 各粒度的标题行高度：年最粗最高，天最细最矮。 */
export function headerHeight(gran: Granularity): number {
  return gran === "year" ? 64 : gran === "month" ? 52 : 44;
}

function aspectRatio(a: AssetRow): number {
  if (a.thumb_w && a.thumb_h && a.thumb_w > 0) {
    const r = a.thumb_h / a.thumb_w;
    return Math.min(MAX_RATIO, Math.max(MIN_RATIO, r));
  }
  return 1;
}

/**
 * 把时间线分组铺成带 y 偏移的行序列。
 *
 * - 普通模式：每组 = header + 若干等边瓦片行（正方形）。
 * - `masonry` 模式（缩放到最大档）：每组 = header + 一组按「最短列优先」摆放的
 *   可变高度瓦片。顺序稳定，分页追加不会移动已放置的项。
 */
export function buildLayout(
  groups: AssetGroup[],
  columns: number,
  tileW: number,
  gap: number,
  remainder: number,
  hasMore: boolean,
  headerH: number,
  masonry: boolean,
): Layout {
  const cols = Math.max(1, columns);
  const size = tileW < 1 ? 1 : tileW;
  const rows: GridRow[] = [];
  const indexY: number[] = [];
  const indexH: number[] = [];
  let y = gap;
  let index = 0;

  for (const g of groups) {
    rows.push({
      type: "header",
      key: g.key,
      label: g.label,
      count: g.assets.length,
      y,
      h: headerH,
    });
    const contentTop = y + headerH;

    if (masonry) {
      // 单列等宽大图流：**每张一个行**，这样可见性裁剪仍然逐张生效
      // （若整组放一行，遇到「一天几千张」会把整组都渲染出来，虚拟化失效）。
      let cy = contentTop;
      for (const a of g.assets) {
        const h = Math.max(1, Math.round(size * aspectRatio(a)));
        const cell: TileCell = { asset: a, index, x: 0, y: cy, w: size, h };
        rows.push({ type: "tiles", key: `${g.key}#${index}`, y: cy, h, cells: [cell] });
        indexY[index] = cy;
        indexH[index] = h;
        cy += h + gap;
        index++;
      }
      y = cy;
    } else {
      const stride = size + gap;
      const nRows = Math.ceil(g.assets.length / cols);
      for (let r = 0; r < nRows; r++) {
        const cells: TileCell[] = [];
        for (let c = 0; c < cols; c++) {
          const i = r * cols + c;
          if (i >= g.assets.length) break;
          const cy = contentTop + r * stride;
          cells.push({
            asset: g.assets[i],
            index: index + i,
            // 余数按 1px 分配到前面的列间，横向铺满、左右贴边（否则全堆在右侧）。
            x: c * (size + gap) + Math.min(c, remainder),
            y: cy,
            w: size,
            h: size,
          });
          indexY[index + i] = cy;
          indexH[index + i] = size;
        }
        rows.push({
          type: "tiles",
          key: `${g.key}#${r}`,
          y: contentTop + r * stride,
          h: stride,
          cells,
        });
      }
      index += g.assets.length;
      y = contentTop + nRows * stride;
    }
  }

  if (hasMore) {
    rows.push({ type: "footer", key: "footer", y, h: FOOTER_H });
    y += FOOTER_H;
  }

  return {
    rows,
    totalH: y,
    indexToY: (i) => indexY[i] ?? 0,
    indexHeight: (i) => indexH[i] ?? size,
  };
}
