# LivePorter 库模型重构设计：平铺库（Flat Library）

- 日期：2026-09-19
- 状态：**待评审**（评审通过前不写实现）
- 关系：本文件**修订** `2026-09-17-liveporter-design.md` 的 §4 目录结构、§5 数据模型、§6 导入管道、附录 A 决策 2/9；冲突处以本文件为准。
- 参考数据：`D:\图片\Pictures`（用户现有相册，实测统计见 §2）。

---

## 1. 背景与问题

现状（v1.0.0）：

- `Library::open(root)` 无条件 `ensure_structure()`，在用户选的目录里创建 `originals/ thumbs/ previews/ larges/ .lpm/`（`src-tauri/src/library.rs:54-65`）。
- 索引只扫 `originals/<设备目录>/<年份>/`（`src-tauri/src/indexer.rs:35-84`）。
- 派生缓存目录对用户直接可见。

后果：把**已有的相册目录**加为库时

1. 软件往用户目录里写一堆可见子目录（污染）；
2. 索引扫不到已有照片，界面空白（不可用）。

这正是本次要重构的原因。

## 2. 参考：真实相册结构实测（`D:\图片\Pictures`）

| 指标 | 值 |
|---|---|
| 文件总数 | 9057 |
| 目录深度（相对根） | 0 层 7、1 层 4769、2 层 3262、3 层 62、4 层 959 |
| 跨目录同名（照片主名） | **933 组** |
| 同目录含 `.heic` + `.mov` 的目录 | 959 |
| 扩展名 | jpg 7066、webp 581、mov 491、heic 468、mp4 357、png 39、jpeg 15、gif 5、psd 18、sai 5、db 4、ini 2、bin 2、`desktop.ini`/`.nomedia`/`.txt` 等 |
| 只读目录 | 2（`Camera Roll`、`Saved Pictures`） |

**结论**：
- 必须**递归**扫描（存在 0–4 层嵌套；根目录本身也有散落文件）。
- 必须**按路径**区分条目（933 组跨目录同名，只按主名会大量互相覆盖）。
- 现有媒体扩展名白名单已能过滤 `.psd/.sai/.db/.ini/.nomedia/desktop.ini` 等垃圾。
- 缓存必须放库根 `.lpm`（只读目录不可依赖写入；Windows 目录的 ReadOnly 属性不阻止写入，但仍以库根为统一落点）。

## 3. 目标 / 非目标

**目标**

- 任意已有照片目录（含用户自己的嵌套子目录）可直接作为库打开、立即浏览；**不改动、不搬迁、不重命名**用户文件。
- 派生缓存与索引在资源管理器里**不可见**。
- 导入仍可用：写入库根、与已有照片同级、按主名配对。

**非目标（本次分期之外）**

- 从磁盘读取拍摄时间（Phase 4，需真实素材实测）。
- iCloud / 「保留原件」自动检测。
- 多库同时打开。

## 4. 决策（新增编号 21–27）

| # | 决策项 | 结论 |
|---|---|---|
| 21 | 库根语义 | 用户指定的目录**就是**照片目录，取消 `originals/` 间接层 |
| 22 | 派生数据位置 | 全部收进库根隐藏目录 `.lpm/`：`index.db` + `thumbs/ previews/ larges/ view/`，并设 Windows 隐藏属性 |
| 23 | 扫描范围 | 递归整棵目录树，**跳过 `.lpm`**；每个目录内按主名配对 |
| 24 | 导入落点 | 库根平铺（`dir=""`）；重名成对加 `_N` 后缀（复用 `unique_names`） |
| 25 | 数据键 | `asset` 以 `(dir, base_name)` 唯一；**移除 `device_id`**；`device` 表降级为导入来源记录（仅供诊断） |
| 26 | 旧库兼容 | 不迁移；旧 `.lpm/index.db`（v1）直接**重建**（索引丢失、重扫即恢复） |
| 27 | 删除语义 | 单张直接删除（进回收站，不弹确认）；**批量（≥2）**弹确认 |
| 28 | 导入去重键 | 增量去重按**导入来源** `(src_serial, src_name)`，不按落盘名——平铺后落盘名可能被加后缀 `_N` |

**对原设计的修改**：决策 2 保留「单一用户目录即库」，但**取消内部 `originals/` 副本**；决策 9 取消「按设备分目录 + 后缀」，改为平铺 + 后缀；§4/§5/§6 按本文件重写。

## 5. 新目录结构

```
<用户选的库根>/
├── IMG_1234.HEIC               ← 用户照片可直接平铺在根
├── IMG_1234.MOV
├── 2023/…                      ← 用户自己的嵌套子目录，递归扫描
├── MI10PRO/2020-08/…           ← 同上
└── .lpm/                       ← 隐藏（FILE_ATTRIBUTE_HIDDEN），资源管理器默认不可见
    ├── index.db
    ├── thumbs/<hash>.webp
    ├── previews/<hash>.mp4
    ├── larges/<hash>.webp
    └── view/<hash>.webp | <hash>-v.mp4
```

- 打开库**不再创建**任何可见子目录；只创建隐藏的 `.lpm/`（及其子目录）。
- `.lpm/` 仍可随时删除（丢索引与缓存，原图完好，重扫即恢复）。

## 6. 数据模型（schema v2）

```sql
CREATE TABLE IF NOT EXISTS device (          -- 仅记录导入来源，供诊断；不参与 asset 键
  id            INTEGER PRIMARY KEY,
  serial        TEXT NOT NULL UNIQUE,
  model         TEXT NOT NULL,
  display_name  TEXT,
  folder_name   TEXT NOT NULL,
  last_seen_at  INTEGER
);

CREATE TABLE IF NOT EXISTS asset (
  id            INTEGER PRIMARY KEY,
  dir           TEXT NOT NULL DEFAULT '',    -- 相对库根的目录（'' = 根），用 '/' 分隔
  base_name     TEXT NOT NULL,               -- 去扩展名的主名
  kind          INTEGER NOT NULL,            -- 1 照片 2 视频 3 实况
  taken_at      INTEGER NOT NULL DEFAULT 0,  -- epoch 秒；0 = 未知
  taken_src     INTEGER NOT NULL DEFAULT 0,  -- 0 unknown 1 mtime 2 exif/meta（Phase 4 用）
  still_path    TEXT, still_ext TEXT, still_size INTEGER,
  movie_path    TEXT, movie_ext TEXT, movie_size INTEGER,
  content_id    TEXT,
  integrity     INTEGER NOT NULL DEFAULT 0,
  -- 导入来源（仅导入条目有）：供增量去重，避免平铺重名加后缀后重复导入
  src_name      TEXT,
  src_serial    TEXT,
  status        INTEGER NOT NULL DEFAULT 0,
  error         TEXT,
  thumb_path    TEXT,
  preview_path  TEXT,
  missing       INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER, updated_at INTEGER,
  UNIQUE(dir, base_name)
);

CREATE INDEX IF NOT EXISTS idx_asset_taken_at   ON asset(taken_at DESC);
CREATE INDEX IF NOT EXISTS idx_asset_kind       ON asset(kind);
CREATE INDEX IF NOT EXISTS idx_asset_integrity  ON asset(integrity);
```

- 迁移：`SCHEMA_VERSION = 2`；检测到 `< 2` 时 `DROP TABLE IF EXISTS asset/device` 后重建（**只支持新结构**）。
- `dir` 由 `still_path`/`movie_path` 相对库根推导；作为稳定键，不随 `taken_at` 变化。
- `device` 表保留仅为诊断（`first_device`）；不再有 `device_id_for_folder`。

## 7. 扫描与配对（`indexer.rs`）

- 从库根**递归**遍历，跳过名为 `.lpm` 的目录；目录内 `collect_files` → `pair_files`（复用 `pairing.rs`）。
- `dir` = 该目录相对库根、统一 `/` 分隔（如 `MI10PRO/2020-08`）。
- upsert 按 `(dir, base_name)`：新增插入；已存在则**保留** `taken_at / taken_src / integrity / status / thumb_path / preview_path / content_id`，只更新路径/大小/kind，并清 `missing`。
- 先 `UPDATE asset SET missing=1`（全局），命中的 upsert 清回 0；扫描结束仍为 1 的即磁盘已不存在。
- 进度事件与取消沿用现有 `scan_library_with`，仅把「按设备/年」改为「按目录」。

## 8. 导入（`importer.rs` / `commands.rs`）

- 目标目录 = **库根**（`asset_dir` 取消 device/year 层级）。
- `unique_names` 不变（重名成对加 `_N`）。数据库键用**落盘主名**；设备原名另存 `src_name` + `src_serial`。
- `diff_tasks` 的「已存在」键用导入来源 `(src_serial, src_name)`；状态/缩略图/内容标识/完整性的写入按 `(dir="", 落盘主名)`。
- 仍 `upsert_device(...)` 记录来源设备（诊断），但 `device` 表不参与 asset 键。

## 9. 协议与安全（`protocol.rs` / `commands.rs`）

- `lpm://` 白名单从「五个子目录」改为「**库根递归**」，但仍要求媒体扩展名（`pairing::is_still_name || is_movie_name`）；`canonicalize` + 根包含判断保留。
- `open_library` 的「拒绝盘根 / 系统目录」（`validate_library_root`）保留。
- `.lpm` 设隐藏属性（`SetFileAttributesW` + `FILE_ATTRIBUTE_HIDDEN`；新增 `windows` crate feature `Win32_Storage_FileSystem`）。

## 10. 拍摄时间（Phase 4）

### 10.1 实测结论（2026-09-19，真实素材）

- 裁剪版 `ffprobe` 能读 **MOV/MP4** 的 `creation_time`（UTC）与 `com.apple.quicktime.creationdate`（带时区 +0800）；
  但 **HEIC 只报 brand、读不到任何时间**，**JPG 的 `format_tags` 里也没有时间**。
- 真实 Android JPG 的 EXIF 里有 `2020:08:04 10:03:03`；部分 Android `mp4`（微信产物）无 `creation_time`。

→ 结论：**静态图必须自写 EXIF 解析**；视频可用 mvhd（自写）或 ffprobe。

### 10.2 取时间来源（优先级）

1. **静态图** `jpg/jpeg/heic/heif`：自写 EXIF —— ExifIFD `DateTimeOriginal`(0x9003) → `DateTimeDigitized`(0x9004) → IFD0 `DateTime`(0x0132)。
2. **视频** `mov/mp4/m4v`：自写 `moov/mvhd` 的 `creation_time`（1904 基准，UTC）。
3. **回退**：文件修改时间（mtime）。
4. 仍无 → `0` / 未知日期。

`taken_src`：`0` unknown、`1` mtime、`2` exif/meta。

### 10.3 时区

- EXIF 无时区 → 按**本地时间**解释为 epoch。
- MOV `mvhd` 为 UTC → 直接换算。（Apple `com.apple.quicktime.creationdate` 带偏移，可后续用于精确本地时刻。）

### 10.4 实现（纯 Rust，避免逐文件起 ffprobe）

- 新增 `time.rs`（或 `exif.rs`）：
  - JPEG：找 `APP1`(0xFFE1) + `Exif\0\0` → TIFF；
  - HEIC：解析 ISO BMFF（`meta`→`iinf`→`iloc` 定位 `Exif` item）→ TIFF；
  - 再按 TIFF IFD 结构取上述 tag。
  - MOV/MP4：扫 `moov`→`mvhd` 取 `creation_time`。
- `indexer` 扫描时对每个媒体文件取时间，写入 `taken_at`/`taken_src`。

### 10.5 重扫

- 已有条目：仅当新来源**优于**旧来源时更新（`exif/meta` > `mtime` > `unknown`），否则保留（与 integrity 同理）。
- 导入条目（来源 WPD，真实时间，`taken_src=2`）不会被扫描覆盖。

> **状态（2026-09-19，已实现）**：`src-tauri/src/time.rs`（EXIF / mvhd / mtime 回退；EXIF 无时区按本机时区解释为真实 epoch）
> 已接入 `indexer`。真实目录实测：`MI10PRO`（Android JPG）与 `iPhone`（HEIC+MOV 实况）拍摄时间均正确，
> 且 HEIC 与同组 MOV 推出的时间一致；已有 meta 时间的条目重扫跳过读文件（2067 条约 0.4s）。


## 11. 影响模块

| 模块 | 改动 |
|---|---|
| `library.rs` | 去掉可见目录与 `originals_dir`；`.lpm` 内建缓存；新增隐藏属性设置 |
| `protocol.rs` | 白名单改库根递归；去掉 `library` 常量依赖 |
| `indexer.rs` | 递归扫描 + 同目录配对 + `dir` 键 |
| `db.rs` | schema v2、迁移、去掉 `device_id`、函数改 `(dir, base_name)`、测试同步 |
| `importer.rs` | 目标改库根平铺 |
| `commands.rs` | 导入/缩略图/分类/诊断的路径与键；`open_library` 不变 |
| `thumb.rs` | 输出目录仍是传入的 `thumbs_dir`（现在指向 `.lpm/thumbs`），基本不动 |
| `delete.rs` | 根内校验 + `.lpm` 缓存清理路径适配 |
| `Cargo.toml` | 增加 windows feature `Win32_Storage_FileSystem` |
| 前端 | 基本不动；删除确认阈值从 ≥10 改为 ≥2（单张不弹确认），见 `App.vue` |
| 文档 | 原设计文档加修订附录；`AGENTS.md` 更新 |

## 12. 分期

- **Phase 1**：`library.rs` + `.lpm` 迁址 + 隐藏属性 + `protocol.rs`。
- **Phase 2**：`db.rs` v2 + `indexer.rs` 递归扫描。
- **Phase 3**：`importer.rs`/`commands.rs` 平铺导入 + 删除始终确认。
- **Phase 4**：拍摄时间提取（需真实素材实测）。
- **Phase 5**：清理遗留 + 设计文档/AGENTS 更新 + 测试。

（本次先做 Phase 1–3；Phase 4/5 之后。）

## 13. 风险登记

| 风险 | 等级 | 说明 / 缓解 |
|---|---|---|
| 递归扫描大目录耗时 | 中 | 沿用异步 + 进度 + 取消；只按文件名配对，不读内容 |
| 库根只读导致 `.lpm` 创建失败 | 低 | Windows 目录 ReadOnly 属性不阻止写入；失败时给出明确错误 |
| 放宽协议白名单 | 中 | 仍限媒体扩展名；`open_library` 拒绝盘根/系统目录 |
| 旧库的 `thumbs/` 被当照片扫入 | 中 | 只跳过 `.lpm`；旧 `originals/thumbs/...` 需用户手动清理（文档说明） |
| schema 变更丢旧索引 | 低 | 重扫可重建；原图/原视频不受影响 |
| 去 `device_id` 后丢失设备归属 | 低 | UI 未展示单条设备归属；`device` 表仍记录来源供诊断 |

## 14. 验收标准

- 指向实测目录（或在 `D:\图片\Pictures\MI10PRO` 这类目录上）打开库：不产生任何**可见**新目录/文件；网格立即显示该目录（含子目录）的媒体。
- 同名不同目录的条目各自独立显示（对应 933 组场景）。
- `.lpm` 在资源管理器默认不可见。
- 导入：文件落到库根、与已有照片同级；重名成对加后缀。
- `lpm://` 拒绝非媒体文件与库外路径。
- `cargo test` / `clippy -D warnings` / `fmt --check` / `vue-tsc` / `vite build` 全绿。
