import type { AssetRow } from "../stores/library";

export type Granularity = "day" | "month" | "year";

export interface AssetGroup {
  key: string; // "2026-09-18" / "2026-09" / "2026" / "unknown"
  label: string; // "2026年9月18日 星期五" / "2026年9月" / "2026年" / "未知日期"
  assets: AssetRow[];
}

const WEEKDAYS = ["日", "一", "二", "三", "四", "五", "六"];
const pad = (n: number) => String(n).padStart(2, "0");

function parts(sec: number) {
  const d = new Date(sec * 1000);
  const y = d.getFullYear();
  const m = d.getMonth() + 1;
  const day = d.getDate();
  return {
    keys: {
      day: `${y}-${pad(m)}-${pad(day)}`,
      month: `${y}-${pad(m)}`,
      year: `${y}`,
    },
    labels: {
      day: `${y}年${m}月${day}日 星期${WEEKDAYS[d.getDay()]}`,
      month: `${y}年${m}月`,
      year: `${y}年`,
    },
  };
}

/** 列数越大 = 缩得越小 = 分段越粗（iOS 式：张开看天，收拢看年）。 */
export function granularityForColumns(columns: number): Granularity {
  if (columns <= 5) return "day";
  if (columns <= 9) return "month";
  return "year";
}

/**
 * 输入已按 taken_at DESC 排好序的条目，按 `gran` 切成连续分段。
 * 同段不会出现两个分组（依赖输入有序）。`taken_at=0` 排在最后、归入「未知日期」。
 */
export function groupAssets(assets: AssetRow[], gran: Granularity): AssetGroup[] {
  const out: AssetGroup[] = [];
  for (const a of assets) {
    let key: string;
    let label: string;
    if (!a.taken_at) {
      key = "unknown";
      label = "未知日期";
    } else {
      const pt = parts(a.taken_at);
      key = pt.keys[gran];
      label = pt.labels[gran];
    }
    const last = out[out.length - 1];
    if (last && last.key === key) last.assets.push(a);
    else out.push({ key, label, assets: [a] });
  }
  return out;
}
