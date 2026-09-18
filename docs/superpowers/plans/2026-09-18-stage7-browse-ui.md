# 阶段 7：浏览体验与 UI（时间线分组 + 搜索筛选 + 多选导出 + 从库删除）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把设计 §9 里除「导入」和「网格基础浏览」之外的四项做完：**时间线分组**（年/月/天多级分段，随缩放松紧切换，带日期头）、**搜索/筛选**（文字、类型、日期区间、完整性）、**多选导出**（原样拷贝配对文件到另一目录）、**从库删除**（移入 Windows 回收站，同步清理索引与缓存）。顺带把网格从「一次性加载前 5000 条」升级为分页增量加载，否则筛选与时间线在大库上会说谎。

**Architecture:** 查询能力集中在 `db.rs`（`AssetFilter` + 条件 SQL，参数全部绑定）；派生/文件操作拆成可测的纯逻辑模块 `export.rs` / `delete.rs`；命令层只做编排。前端把网格布局抽成纯函数 `src/lib/gridLayout.ts`（可独立推理），`PhotoGrid.vue` 负责虚拟化渲染，筛选状态与选中集合放进 Pinia `library` store。

**Tech Stack:** Rust 2021 / rusqlite 0.40 / `trash` 5（新增，Windows 回收站）/ Tauri 2 / Vue 3 + TS + Pinia

**为什么现在做（设计 §13 第 7 步、§9）：** 阶段 1~6 已完成导入、索引、缩略图、预览片、完整性校验。浏览界面目前只有「按主名排序的扁平网格 + 前 5000 条」，既没有时间线，也没有筛选，更不能导出/删除。

---

## 关键事实（2026-09-18 查证，勿凭记忆改）

1. **回收站库选型**：`trash` crate（crates.io 最新 **5.2.7**，docs.rs 明确 Windows 走系统回收站）。
   API：`trash::delete(path)` / `trash::delete_all(&[paths])`，返回 `Result<(), trash::Error>`。
   **Task 0 必须在本机先做一次真实验证**，不要只信文档。
2. **Tauri 2 命令参数命名**：Rust `snake_case` 形参在 JS 侧用 `camelCase` 传入（既有铁证：`import_from_device(assume_cloud)` 在 `src/stores/import.ts:30` 用 `{ assumeCloud }` 调用）。**嵌套结构体字段不参与这个转换**，走 serde 原名——所以过滤器字段一律用单词命名（`text/kind/integrity/from/to`），避免歧义。
3. **`taken_at` 单位是 epoch 秒**（`db.rs` 全表如此；`pairing`/`importer` 写入来自设备元数据）。时间线分组按**本地时区**换算到「天」。
4. **旧的扁平查询按 `base_name` 排序**（`db.rs:384`），与时间线语义冲突，必须改为 `taken_at DESC, id DESC`；`idx_asset_taken_at` 已存在，能吃到。
5. **缓存文件按内容哈希命名、可被多条复用**（设计 §7.2、AGENTS「阶段 5 关键实现要点」）。删除缩略图/预览片前必须查「是否还有别的条目引用同一路径」，否则会误删共享缓存。
6. **导出/重名后缀规则**（设计 §5.3）：真重复跳过，假重复加后缀（`IMG_0001_1.HEIC` + `IMG_0001_1.MOV`），**配对的两个文件必须用同一个后缀**。
7. **重扫会把导入条目重复插入**（`notes/2026-09-18-integrity-smoke.md` 已记录的「历史重复行」）：`indexer` 用 `taken_at=0` upsert，而导入条目的真 `taken_at` 非 0，业务键 `(device_id, base_name, taken_at)` 不同 → 新增行。**时间线依赖 `taken_at`，本计划必须先修这个**（Task 2）。
8. **回收站有容量配额**：超出时大文件会被永久删除（设计 §10.3），UI 的确认文案要如实说明，不能承诺一定可恢复。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. `trash` 5.2.7 在本机（Win10/11）对普通文件是否成功移入回收站、失败时返回什么（Task 0）。
2. `trash` 与项目既有 `windows 0.58` 是否在同一编译单元冲突（Task 0 用 `cargo build` 验证）。
3. `taken_at=0`（重扫建库、无设备元数据的条目）在时间线里的展示：本计划归入「未知日期」桶。**是否可接受需过审。**
4. 全库「全选」可能产生十万级 id 数组走 IPC；体积估算需在实测时记录（Task 8）。

---

## 文件结构

```
src-tauri/src/
├── db.rs         修改：AssetRow 加 taken_at、排序改 taken_at DESC、AssetFilter/条件查询/count/ids/by_ids、删除行与引用计数
├── indexer.rs    修改：重扫保留既有 taken_at，避免重复行
├── export.rs     新增：导出路径去重、配对拷贝（纯逻辑 + FS）
├── delete.rs     新增：回收站删除编排、缓存引用计数后的清理
├── commands.rs   修改：list_assets(过滤)/count_assets/list_asset_ids/export_assets/delete_assets
└── lib.rs        修改：注册命令
src/
├── lib/timeline.ts          新增：年/月/天多级分组纯函数
├── lib/gridLayout.ts        新增：分组行布局纯函数（时间线头 + 瓦片行 + 页脚）
├── components/PhotoGrid.vue 重写：接收分组、日期头、选中态、near-end
├── components/FilterBar.vue 新增：搜索/类型/日期/完整性
├── stores/library.ts        修改：过滤状态、分页、选中、导出/删除动作
└── App.vue                  修改：工具栏、分组计算、动作编排、确认弹窗
src-tauri/Cargo.toml         修改：加 `trash = "5"`
```

---

## Task 0: 闸门——验证 `trash` crate 真能移入回收站

**Files:** Modify `src-tauri/Cargo.toml`；Create `src-tauri/src/delete.rs`（先放验证用）

- [ ] **Step 1: 加依赖并编译**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 末尾加：
```toml
trash = "5"
```
Run: `cargo build -p liveporter`
Expected: 编译通过（若与 `windows 0.58` 冲突，停下来报告，不要硬改版本）。

- [ ] **Step 2: 写一个 ignored 的真实回收站冒烟测试**

```rust
//! 从库删除：移入 Windows 回收站并同步清理索引与缓存。
//! 真实的回收站调用放在 ignored 测试里（会动系统回收站），默认测试只测纯逻辑。

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "writes to the real recycle bin"]
    fn trash_moves_a_temp_file() {
        let dir = std::env::temp_dir().join("lpm_trash_probe");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("lpm-probe.txt");
        std::fs::write(&f, b"x").unwrap();

        trash::delete(&f).unwrap();
        assert!(!f.exists(), "文件应已从原位置消失");
    }
}
```

- [ ] **Step 3: 运行验证**

Run: `cargo test -p liveporter trash_moves_a_temp_file -- --ignored --nocapture`
Expected: 通过。**把结果记到 Task 8 的 notes。**

- [ ] **Step 4: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/delete.rs src-tauri/src/lib.rs
git commit -m "chore(app): add trash crate and verify recycle-bin deletion"
```
> 记得在 `lib.rs` 加 `mod delete;`（否则文件不参与编译）。

---

## Task 1: 查询层——`taken_at`、排序、过滤器（纯逻辑 + 单测）

**Files:** Modify `src-tauri/src/db.rs`

- [ ] **Step 1: 写失败的测试**

```rust
#[test]
fn escape_like_escapes_wildcards() {
    assert_eq!(escape_like("a%b_c\\d"), "a\\%b\\_c\\\\d");
}

#[test]
fn filter_by_kind_integrity_and_date() {
    let conn = open_in_memory().unwrap();
    let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
    seed(&conn, dev, "IMG_1", 100, KIND_LIVE, 0);
    seed(&conn, dev, "IMG_2", 200, KIND_PHOTO, 3);
    seed(&conn, dev, "IMG_3", 300, KIND_VIDEO, 4);

    let f = AssetFilter { kind: Some(KIND_PHOTO), ..Default::default() };
    let rows = page_assets_filtered(&conn, &f, 0, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].base_name, "IMG_2");

    let f = AssetFilter { integrity: Some(vec![3, 4]), ..Default::default() };
    assert_eq!(page_assets_filtered(&conn, &f, 0, 10).unwrap().len(), 2);

    let f = AssetFilter { from: Some(150), to: Some(299), ..Default::default() };
    let rows = page_assets_filtered(&conn, &f, 0, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].base_name, "IMG_2");
}

#[test]
fn filter_text_matches_substring_and_escapes() {
    let conn = open_in_memory().unwrap();
    let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
    seed(&conn, dev, "IMG_0001", 1, KIND_PHOTO, 3);
    seed(&conn, dev, "IMG_1002", 2, KIND_PHOTO, 3);

    let f = AssetFilter { text: Some("IMG_0".into()), ..Default::default() };
    assert_eq!(page_assets_filtered(&conn, &f, 0, 10).unwrap().len(), 1);
    // `_` 必须当字面量，不能当 LIKE 通配符
    let f = AssetFilter { text: Some("IMG_0".into()), ..Default::default() };
    assert!(page_assets_filtered(&conn, &f, 0, 10).unwrap()[0].base_name == "IMG_0001");
}

#[test]
fn page_orders_by_taken_at_desc_then_id_desc() {
    let conn = open_in_memory().unwrap();
    let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
    seed(&conn, dev, "OLD", 100, KIND_PHOTO, 3);
    seed(&conn, dev, "NEW", 300, KIND_PHOTO, 3);
    seed(&conn, dev, "MID", 200, KIND_PHOTO, 3);

    let names: Vec<_> = page_assets_filtered(&conn, &AssetFilter::default(), 0, 10)
        .unwrap()
        .into_iter()
        .map(|r| r.base_name)
        .collect();
    assert_eq!(names, vec!["NEW", "MID", "OLD"]);
    assert_eq!(page_assets_filtered(&conn, &AssetFilter::default(), 0, 10).unwrap()[0].taken_at, 300);
}

#[test]
fn count_and_ids_match_the_filtered_page() {
    let conn = open_in_memory().unwrap();
    let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
    seed(&conn, dev, "A", 1, KIND_PHOTO, 3);
    seed(&conn, dev, "B", 2, KIND_LIVE, 0);

    let f = AssetFilter { integrity: Some(vec![0]), ..Default::default() };
    assert_eq!(count_assets(&conn, &f).unwrap(), 1);
    assert_eq!(asset_ids(&conn, &f).unwrap().len(), 1);
    assert_eq!(page_assets_filtered(&conn, &f, 0, 10).unwrap().len(), 1);
}
```

测试辅助（放在 `tests` 模块里）：
```rust
fn seed(conn: &Connection, dev: i64, name: &str, taken_at: i64, kind: i64, integrity: i64) {
    let a = PairedAsset {
        base_name: name.into(),
        kind,
        integrity,
        still: Some(FileRef { path: format!("{name}.JPG"), ext: "jpg".into(), size: 10 }),
        movie: if kind == KIND_LIVE {
            Some(FileRef { path: format!("{name}.MOV"), ext: "mov".into(), size: 20 })
        } else {
            None
        },
    };
    upsert_asset(conn, dev, &a, taken_at).unwrap();
}
```

- [ ] **Step 2: 实现**

```rust
/// 转义 SQLite LIKE 的通配符（`%` `_` `\`），配合 `ESCAPE '\'` 使用。
pub fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '%' | '_' => { out.push('\\'); out.push(c); }
            _ => out.push(c),
        }
    }
    out
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AssetFilter {
    /// 按 base_name 子串匹配（大小写不敏感由 SQLite 默认的 ASCII 规则处理）。
    pub text: Option<String>,
    /// 1=photo 2=video 3=live。
    pub kind: Option<i64>,
    /// 完整性白名单；空或 None 表示不过滤。
    pub integrity: Option<Vec<i64>>,
    /// 拍摄时间下界（epoch 秒，含）。
    pub from: Option<i64>,
    /// 拍摄时间上界（epoch 秒，含）。
    pub to: Option<i64>,
}

fn where_clause(f: &AssetFilter, params: &mut Vec<rusqlite::types::Value>) -> String {
    use rusqlite::types::Value;
    let mut clauses: Vec<String> = Vec::new();

    if let Some(t) = f.text.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        clauses.push("base_name LIKE ? ESCAPE '\\'".to_string());
        params.push(Value::Text(format!("%{}%", escape_like(t))));
    }
    if let Some(k) = f.kind {
        clauses.push("kind = ?".to_string());
        params.push(Value::Integer(k));
    }
    if let Some(list) = f.integrity.as_ref().filter(|l| !l.is_empty()) {
        let ph = vec!["?"; list.len()].join(",");
        clauses.push(format!("integrity IN ({ph})"));
        for v in list {
            params.push(Value::Integer(*v));
        }
    }
    if let Some(from) = f.from {
        clauses.push("taken_at >= ?".to_string());
        params.push(Value::Integer(from));
    }
    if let Some(to) = f.to {
        clauses.push("taken_at <= ?".to_string());
        params.push(Value::Integer(to));
    }
    if clauses.is_empty() {
        "1=1".to_string()
    } else {
        clauses.join(" AND ")
    }
}

fn map_asset_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AssetRow> {
    Ok(AssetRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        integrity: r.get(2)?,
        taken_at: r.get(3)?,
        base_name: r.get(4)?,
        still_path: r.get(5)?,
        movie_path: r.get(6)?,
        thumb_path: r.get(7)?,
        missing: r.get::<_, i64>(8)? != 0,
    })
}

pub fn page_assets_filtered(
    conn: &Connection,
    filter: &AssetFilter,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<AssetRow>> {
    let mut params = Vec::new();
    let w = where_clause(filter, &mut params);
    let sql = format!(
        "SELECT id, kind, integrity, taken_at, base_name, still_path, movie_path, thumb_path, missing
         FROM asset WHERE {w} ORDER BY taken_at DESC, id DESC LIMIT ? OFFSET ?"
    );
    params.push(rusqlite::types::Value::Integer(limit));
    params.push(rusqlite::types::Value::Integer(offset));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), map_asset_row)?;
    rows.collect()
}

pub fn count_assets(conn: &Connection, filter: &AssetFilter) -> rusqlite::Result<i64> {
    let mut params = Vec::new();
    let w = where_clause(filter, &mut params);
    let sql = format!("SELECT count(*) FROM asset WHERE {w}");
    conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| r.get(0))
}

pub fn asset_ids(conn: &Connection, filter: &AssetFilter) -> rusqlite::Result<Vec<i64>> {
    let mut params = Vec::new();
    let w = where_clause(filter, &mut params);
    let sql = format!("SELECT id FROM asset WHERE {w} ORDER BY taken_at DESC, id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(0))?;
    rows.collect()
}
```

同时：
- 给 `AssetRow` 加 `pub taken_at: i64`（`id` 之后）。
- 旧 `page_assets` 保留还是删除？**删除**，并把唯一调用点 `commands::list_assets` 改为 `page_assets_filtered`（Task 3）。删除后 `page_and_stats_work` 测试要改用新函数。

- [ ] **Step 3: 跑测试**

Run: `cargo test -p liveporter db`
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/db.rs
git commit -m "feat(app): filtered asset queries ordered by taken_at"
```

---

## Task 2: 重扫保留既有 `taken_at`（修掉时间线/重复行的隐患）

**Files:** Modify `src-tauri/src/db.rs`、`src-tauri/src/indexer.rs`

- [ ] **Step 1: 写失败的测试**

```rust
#[test]
fn rescan_preserves_imported_taken_at() {
    let conn = open_in_memory().unwrap();
    let dev = upsert_device(&conn, "SN1", "m", None, "d").unwrap();
    seed(&conn, dev, "IMG_1", 12345, KIND_PHOTO, 3);

    // 模拟重扫：目录扫描拿不到拍摄时间，传入 0
    let a = PairedAsset {
        base_name: "IMG_1".into(),
        kind: KIND_PHOTO,
        integrity: INTEGRITY_STILL_ONLY,
        still: Some(FileRef { path: "IMG_1.JPG".into(), ext: "jpg".into(), size: 10 }),
        movie: None,
    };
    upsert_asset_preserving_taken_at(&conn, dev, &a).unwrap();

    let n: i64 = conn.query_row("SELECT count(*) FROM asset", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "重扫不得因 taken_at=0 新增重复行");
    let t: i64 = conn.query_row("SELECT taken_at FROM asset", [], |r| r.get(0)).unwrap();
    assert_eq!(t, 12345, "已有条目的 taken_at 必须保留");
}
```

- [ ] **Step 2: 实现**

在 `db.rs` 加：
```rust
/// 重扫专用：同一 (device_id, base_name) 若已有条目，沿用其 taken_at；否则用 0。
/// 这样 `indexer`（拿不到拍摄时间）不会把导入条目复制成第二行。
pub fn upsert_asset_preserving_taken_at(
    conn: &Connection,
    device_id: i64,
    asset: &PairedAsset,
) -> rusqlite::Result<()> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT taken_at FROM asset WHERE device_id = ?1 AND base_name = ?2
             ORDER BY taken_at DESC LIMIT 1",
            params![device_id, asset.base_name],
            |r| r.get(0),
        )
        .optional()?;
    upsert_asset(conn, device_id, asset, existing.unwrap_or(0))
}
```
把 `indexer.rs:51` 的 `crate::db::upsert_asset(conn, device_id, &asset, 0)?` 换成
`crate::db::upsert_asset_preserving_taken_at(conn, device_id, &asset)?`。

> 注意：这**不能**消除历史遗留的重复行（那些行 `taken_at` 已不同），只能阻止继续产生。清库重导仍是彻底办法（与 `integrity-smoke` notes 结论一致）。

- [ ] **Step 3: 跑测试**

Run: `cargo test -p liveporter`
Expected: 通过（indexer 既有测试不受影响）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/db.rs src-tauri/src/indexer.rs
git commit -m "fix(app): preserve taken_at on rescan to avoid duplicate rows"
```

---

## Task 3: 命令层与前端 store——过滤、分页、选中

**Files:** Modify `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`、`src/stores/library.ts`

- [ ] **Step 1: 后端命令**

`commands.rs`：
```rust
#[tauri::command]
pub fn list_assets(
    offset: i64,
    limit: i64,
    filter: Option<crate::db::AssetFilter>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::db::AssetRow>, String> {
    let f = filter.unwrap_or_default();
    state
        .with(|_, conn| crate::db::page_assets_filtered(conn, &f, offset, limit))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_assets(
    filter: Option<crate::db::AssetFilter>,
    state: tauri::State<'_, AppState>,
) -> Result<i64, String> {
    let f = filter.unwrap_or_default();
    state
        .with(|_, conn| crate::db::count_assets(conn, &f))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_asset_ids(
    filter: Option<crate::db::AssetFilter>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<i64>, String> {
    let f = filter.unwrap_or_default();
    state
        .with(|_, conn| crate::db::asset_ids(conn, &f))
        .ok_or_else(|| "库未打开".to_string())?
        .map_err(|e| e.to_string())
}
```
`lib.rs` 的 `generate_handler!` 里加这三项。

- [ ] **Step 2: 前端 store**

改 `src/stores/library.ts`：新增过滤状态、分页与选中：
```ts
export interface AssetFilter {
  text: string;
  kind: number | null;
  integrity: number[];
  from: string | null; // "YYYY-MM-DD"
  to: string | null;   // "YYYY-MM-DD"
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
```
store 内：
```ts
const filter = reactive<AssetFilter>({ text: "", kind: null, integrity: [], from: null, to: null });
const total = ref(0);
const loading = ref(false);
const selected = ref<Set<number>>(new Set());

function setSelected(next: Set<number>) { selected.value = next; } // 重新赋值，确保响应

async function reload() {
  loading.value = true;
  error.value = null;
  try {
    const f = backendFilter(filter);
    assets.value = await invoke<AssetRow[]>("list_assets", { offset: 0, limit: PAGE_SIZE, filter: f });
    total.value = await invoke<number>("count_assets", { filter: f });
    setSelected(new Set());
  } catch (e) { error.value = String(e); }
  finally { loading.value = false; }
}

async function loadMore() {
  if (loading.value || assets.value.length >= total.value) return;
  loading.value = true;
  try {
    const f = backendFilter(filter);
    const next = await invoke<AssetRow[]>("list_assets", {
      offset: assets.value.length, limit: PAGE_SIZE, filter: f,
    });
    assets.value = [...assets.value, ...next]; // shallowRef 必须整体替换
  } catch (e) { error.value = String(e); }
  finally { loading.value = false; }
}

function toggleSelect(id: number) {
  const next = new Set(selected.value);
  next.has(id) ? next.delete(id) : next.add(id);
  setSelected(next);
}

async function selectAll() {
  const ids = await invoke<number[]>("list_asset_ids", { filter: backendFilter(filter) });
  setSelected(new Set(ids));
}

function clearSelection() { setSelected(new Set()); }
```
`refresh()`/`openLibrary()`/`rescan()` 成功后改为调用 `reload()`。`AssetRow` 加 `taken_at: number`。

- [ ] **Step 3: 构建检查**

Run: `npx vue-tsc --noEmit`
Expected: 通过（此时 App.vue 还在用旧 `list_assets` 的三参调用？不会——命令签名向后兼容 `offset/limit` + `filter`，`invoke` 传 `filter` 即可）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/stores/library.ts
git commit -m "feat(app): filtered paginated asset queries and selection state"
```

---

## Task 4: 时间线分组与网格布局纯函数

**Files:** Create `src/lib/gridLayout.ts`；Create `src/lib/timeline.ts`

- [ ] **Step 1: 分组纯函数 `timeline.ts`（多级）**

```ts
import type { AssetRow } from "../stores/library";

export type Granularity = "day" | "month" | "year";

export interface AssetGroup {
  key: string;   // "2026-09-18" / "2026-09" / "2026" / "unknown"
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
```
> 切换粒度**只重新分组已加载的条目**，不重新查询；因为后端始终按 `taken_at DESC` 返回，任意粒度下同段都连续。
> 本计划的分级是「同一网格换分段标题」，不是 iOS 那种年/月缩略图拼贴页；后者属后续增强。

- [ ] **Step 2: 布局纯函数 `gridLayout.ts`**

```ts
import type { AssetGroup } from "./timeline";

export interface TileCell { asset: import("../stores/library").AssetRow; index: number; x: number; size: number }
export type GridRow =
  | { type: "header"; key: string; label: string; y: number; h: number }
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
export function headerHeight(gran: "day" | "month" | "year"): number {
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
  const stride = (tileW < 1 ? 1 : tileW) + gap;
  const rows: GridRow[] = [];
  const groupStartY: number[] = [];
  const groupStartIndex: number[] = [];
  let y = gap;
  let index = 0;

  for (const g of groups) {
    groupStartY.push(y);
    groupStartIndex.push(index);
    rows.push({ type: "header", key: g.key, label: g.label, y, h: headerH });
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
          size: tileW < 1 ? 1 : tileW,
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
```
> `buildLayout` 只依赖已加载的 `groups`。追加分页时 `hasMore` 为真，页脚提供滚动余量并触发 `near-end`。

- [ ] **Step 3: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 通过（尚未接入组件，仅新文件）。

- [ ] **Step 4: 提交**

```bash
git add src/lib/timeline.ts src/lib/gridLayout.ts
git commit -m "feat(app): timeline grouping and grid layout pure functions"
```

---

## Task 5: 重写 `PhotoGrid.vue`——日期头 + 选中态 + near-end

**Files:** Modify `src/components/PhotoGrid.vue`、`src/App.vue`

- [ ] **Step 1: 组件接口**

Props：
```ts
{
  groups: AssetGroup[];
  columns: number;
  gap: number;
  granularity: Granularity;
  selected: Set<number>;
  hasMore: boolean;
}
```
Emits：`hover`（同前）、`hover-out`、`suppress`、`toggle-select: [id: number]`、`near-end: []`。

- [ ] **Step 2: 渲染逻辑**

- `viewportW/H` 与 `tileW` 计算不变（`tileW` 仍按容器宽与列数算）。
- `const hh = computed(() => headerHeight(props.granularity));`
- `layout = computed(() => buildLayout(props.groups, props.columns, tileW.value, props.gap, props.hasMore, hh.value))`。
- 日期头按粒度设置字号：年最大、天最小（用 `hh.value` 或 class 即可）。
- `visibleRows`：对 `layout.rows` 做线性过滤：
  ```ts
  const top = scrollTop.value;
  const bottom = top + viewportH.value;
  const visibleRows = computed(() =>
    layout.value.rows.filter((r) => r.y + r.h >= top - 2 && r.y <= bottom + 2),
  );
  ```
  （行数 ~ 千级，每帧线性过滤开销可忽略；如后续嫌慢再换二分。）
- 模板里 `v-for` 遍历 `visibleRows`，按 `row.type` 分支渲染 header / tiles / footer。
- 瓦片 `path = asset.thumb_path || asset.still_path`；沿用占位 GIF 与滚动停稳后再加载的逻辑。
- 选中态：`class="{ selected: props.selected.has(asset.id) }"`，右上角一个勾选标记；点击 `emit("toggle-select", asset.id)`。
- 缩放锚点：`captureAnchor` 用 `indexToY` 的反向思路——从 `localY` 找到命中的行，`index = row.cells[col].index`；`restoreAnchor` 用 `layout.indexToY(anchor.index)` 设 `scrollTop`。
  > 若命中的是 header/footer 行，回退到最近的下一个瓦片行的第一格。

- [ ] **Step 3: `App.vue` 接线**

```ts
const gridAssets = computed(() => lib.assets.filter((a) => !a.missing && (a.thumb_path || a.still_path)));
// 粒度随缩放列数变化：张开看天、收拢看年（设计 §9 + 本次决策）。
const gran = computed(() => granularityForColumns(zoom.columns.value));
const groups = computed(() => groupAssets(gridAssets.value, gran.value));

function onNearEnd() { void lib.loadMore(); }
function onToggleSelect(id: number) { lib.toggleSelect(id); }
```
模板：
```vue
<PhotoGrid
  :groups="groups"
  :columns="zoom.columns.value"
  :gap="8"
  :granularity="gran"
  :selected="lib.selected"
  :has-more="lib.assets.length < lib.total"
  @hover="onHover" @hover-out="onHoverOut" @suppress="onSuppress"
  @toggle-select="onToggleSelect" @near-end="onNearEnd"
/>
```
> `hasMore` 用 `assets.length < total` 判断；`total` 是过滤后的总数，不是 `stats.total`。
> 缩放换粒度时 `groups` 重算、`layout` 重算，锚点仍按全局序号还原，因此视图不跳。

- [ ] **Step 4: 检查**

Run: `npx vue-tsc --noEmit; npm run build`
Expected: 通过。

- [ ] **Step 5: 提交**

```bash
git add src/components/PhotoGrid.vue src/App.vue
git commit -m "feat(app): timeline headers, selection and infinite scroll in grid"
```

---

## Task 6: 筛选栏 UI

**Files:** Create `src/components/FilterBar.vue`；Modify `src/App.vue`

- [ ] **Step 1: 组件**

`FilterBar.vue`（`v-model` 绑定 `lib.filter` 的字段，或在组件内 `emit("change")` 由 App 调 `lib.reload()`）：
- 文本输入：`placeholder="搜索文件名"`，**输入防抖 250ms** 后 `emit("change")`。
- 类型：三个切换（全部 / 实况 / 照片 / 视频），映射 `kind = null | 3 | 1 | 2`。
- 日期：`<input type="date">` × 2（起、止）。
- 完整性：多选 chips —— 全部 / 正常(0) / 不一致(1) / 残缺(2) / 仅静态(3) / 仅视频(4) / 疑似副本(5)。选中集合写入 `integrity: number[]`。
- 分段粒度：`自动（跟随缩放）/ 年 / 月 / 天` 四档，写入 `lib.granOverride`。**不触发 reload**，只重组标题。
- 一行「已加载 X / 共 Y」与「清空筛选」。

- [ ] **Step 2: App 接线**

`App.vue` 头部下方插入 `<FilterBar @change="lib.reload" />`。`lib.reload` 里会清空选中（过滤条件变了，旧选中已不可见）。
在 `library.ts` 加 `const granOverride = ref<"auto" | Granularity>("auto");`，`App.vue` 的粒度计算改为：
```ts
const gran = computed<Granularity>(() =>
  lib.granOverride === "auto" ? granularityForColumns(zoom.columns.value) : lib.granOverride,
);
```
> 防抖只包住文本输入；类型/日期/完整性/粒度是离散操作，立即生效（粒度不查库）。

- [ ] **Step 3: 检查**

Run: `npx vue-tsc --noEmit; npm run build`
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src/components/FilterBar.vue src/App.vue
git commit -m "feat(app): search and filter bar for the asset grid"
```

---

## Task 7: 多选导出（原样拷贝配对文件）

**Files:** Create `src-tauri/src/export.rs`；Modify `src-tauri/src/db.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`、`src/stores/library.ts`、`src/App.vue`

- [ ] **Step 1: 纯逻辑 + 单测（`export.rs`）**

```rust
use std::path::{Path, PathBuf};

/// 在目标目录里找一个不冲突的「主名」，返回配对的静态图/视频目标路径。
/// 规则同设计 §5.3：首个可用主名是原名，之后依次 `_1`、`_2`……
/// 配对的两个文件必须用同一个后缀，否则实况配对被拆开。
pub fn unique_export_paths(
    dest_dir: &Path,
    base: &str,
    still_ext: Option<&str>,
    movie_ext: Option<&str>,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut n = 0u32;
    loop {
        let stem = if n == 0 { base.to_string() } else { format!("{base}_{n}") };
        let still = still_ext.map(|e| dest_dir.join(format!("{stem}.{e}")));
        let movie = movie_ext.map(|e| dest_dir.join(format!("{stem}.{e}")));
        let free = still.as_ref().map_or(true, |p| !p.exists())
            && movie.as_ref().map_or(true, |p| !p.exists());
        if free {
            return (still, movie);
        }
        n += 1;
        if n > 10_000 {
            return (still, movie); // 极端兜底：返回最后一个候选，由调用方处理覆盖风险
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_base_name_when_free() {
        let dir = std::env::temp_dir().join("lpm_export_t1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (s, m) = unique_export_paths(&dir, "IMG_1", Some("heic"), Some("mov"));
        assert_eq!(s.unwrap().file_name().unwrap(), "IMG_1.heic");
        assert_eq!(m.unwrap().file_name().unwrap(), "IMG_1.mov");
    }

    #[test]
    fn adds_same_suffix_to_both_when_colliding() {
        let dir = std::env::temp_dir().join("lpm_export_t2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("IMG_1.heic"), b"x").unwrap();
        let (s, m) = unique_export_paths(&dir, "IMG_1", Some("heic"), Some("mov"));
        assert_eq!(s.unwrap().file_name().unwrap(), "IMG_1_1.heic");
        assert_eq!(m.unwrap().file_name().unwrap(), "IMG_1_1.mov");
    }
}
```

- [ ] **Step 2: db 取条目文件路径**

```rust
#[derive(Debug, Clone)]
pub struct AssetFiles {
    pub id: i64,
    pub base_name: String,
    pub still_path: Option<String>,
    pub still_ext: Option<String>,
    pub movie_path: Option<String>,
    pub movie_ext: Option<String>,
    pub thumb_path: Option<String>,
    pub preview_path: Option<String>,
}

/// 按 id 批量取条目文件信息（导出/删除共用）。顺序不保证。
pub fn assets_by_ids(conn: &Connection, ids: &[i64]) -> rusqlite::Result<Vec<AssetFiles>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ph = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT id, base_name, still_path, still_ext, movie_path, movie_ext, thumb_path, preview_path
         FROM asset WHERE id IN ({ph})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok(AssetFiles {
            id: r.get(0)?,
            base_name: r.get(1)?,
            still_path: r.get(2)?,
            still_ext: r.get(3)?,
            movie_path: r.get(4)?,
            movie_ext: r.get(5)?,
            thumb_path: r.get(6)?,
            preview_path: r.get(7)?,
        })
    })?;
    rows.collect()
}
```
测试：插入两条，`assets_by_ids(&[id1])` 只返回一条；空数组返回空。

- [ ] **Step 3: 导出命令**

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportSummary { pub total: usize, pub done: usize, pub failed: usize, pub errors: Vec<String> }

#[tauri::command]
pub async fn export_assets(
    app: tauri::AppHandle,
    ids: Vec<i64>,
    dest: String,
    state: tauri::State<'_, AppState>,
) -> Result<ExportSummary, String> { ... }
```
实现（`spawn_blocking`）：
- `let lib = Library::open(&root)?; let conn = db::open(...)?;`
- `let files = db::assets_by_ids(&conn, &ids)?;`
- 目标目录必须存在且是目录，否则报错。
- 逐条：
  - `unique_export_paths(dest, &base_name, still_ext, movie_ext)`；
  - 对每个存在的源文件 `std::fs::copy`，累计 `bytes_done`；
  - 每条完成后 `app.emit("export://progress", p)`；用与导入相同的取消标志（`state.cancel_flag()`）在文件之间检查。
- 返回 `ExportSummary`（失败项把 `src -> 原因` 塞进 `errors`，最多 50 条）。
- **不**修改索引（导出是只读操作）。

- [ ] **Step 4: 前端动作**

`library.ts`：
```ts
const exporting = ref(false);
const exportProgress = ref<{ total: number; done: number; failed: number } | null>(null);

async function exportSelected(dest: string) {
  const ids = [...selected.value];
  if (!ids.length) return;
  exporting.value = true;
  const un = await listen<typeof exportProgress.value>("export://progress", (e) => (exportProgress.value = e.payload));
  try {
    const r = await invoke<{ total: number; done: number; failed: number }>("export_assets", { ids, dest });
    if (r.failed) error.value = `导出完成，失败 ${r.failed}`;
  } catch (e) { error.value = String(e); }
  finally { un(); exporting.value = false; exportProgress.value = null; }
}
```
`App.vue`：选中工具栏加「导出选中」，用 `open({ directory: true })` 选目标目录（`@tauri-apps/plugin-dialog` 已在用）。

- [ ] **Step 5: 测试与构建**

Run: `cargo test -p liveporter export`；`npx vue-tsc --noEmit; npm run build`
Expected: 通过。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/export.rs src-tauri/src/db.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/stores/library.ts src/App.vue
git commit -m "feat(app): export selected assets as paired original copies"
```

---

## Task 8: 从库删除（回收站 + 索引/缓存清理）

**Files:** Modify `src-tauri/src/delete.rs`、`src-tauri/src/db.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`、`src/stores/library.ts`、`src/App.vue`

- [ ] **Step 1: db 辅助**

```rust
/// 删除指定 id 的条目，返回实际删除行数。
pub fn delete_assets(conn: &Connection, ids: &[i64]) -> rusqlite::Result<usize> {
    if ids.is_empty() { return Ok(0); }
    let ph = vec!["?"; ids.len()].join(",");
    let sql = format!("DELETE FROM asset WHERE id IN ({ph})");
    conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
}

/// 还有多少「其它」条目引用同一个缩略图路径。
pub fn thumb_ref_count(conn: &Connection, path: &str) -> rusqlite::Result<i64> { ... }
/// 同上，预览片。
pub fn preview_ref_count(conn: &Connection, path: &str) -> rusqlite::Result<i64> { ... }
```
测试：同一 `thumb_path` 被两条引用，删掉一条后 `thumb_ref_count == 1`，删完两条后为 0。

- [ ] **Step 2: `delete.rs` 编排 + 纯逻辑单测**

```rust
/// 删除一个条目涉及的文件：静态图 + 视频。
pub fn source_paths(a: &crate::db::AssetFiles) -> Vec<String> {
    [a.still_path.clone(), a.movie_path.clone()].into_iter().flatten().collect()
}

/// 删除后是否可安全移除缓存文件：引用计数为 0。
pub fn cache_is_orphan(ref_count: i64) -> bool { ref_count == 0 }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_paths_includes_both_halves_of_a_live_photo() {
        let a = crate::db::AssetFiles { /* still + movie */
            id: 1, base_name: "IMG_1".into(),
            still_path: Some("s.heic".into()), still_ext: Some("heic".into()),
            movie_path: Some("m.mov".into()), movie_ext: Some("mov".into()),
            thumb_path: None, preview_path: None,
        };
        assert_eq!(source_paths(&a), vec!["s.heic", "m.mov"]);
    }
    #[test]
    fn only_unreferenced_cache_is_removed() {
        assert!(cache_is_orphan(0));
        assert!(!cache_is_orphan(1));
    }
}
```

- [ ] **Step 3: 删除命令**

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeleteSummary { pub total: usize, pub deleted: usize, pub failed: usize, pub errors: Vec<String> }

#[tauri::command]
pub async fn delete_assets(
    app: tauri::AppHandle,
    ids: Vec<i64>,
    state: tauri::State<'_, AppState>,
) -> Result<DeleteSummary, String> { ... }
```
实现（`spawn_blocking`）：
1. 取 `assets_by_ids`。
2. 对每条：把存在且位于库根内的源文件（`still_path`/`movie_path`）交给 `trash::delete`；记录成功/失败与错误原文。
   - 文件已不存在 → 视为成功（本条可删）。
   - 路径在库根外 → 拒绝并记错误（防御性，正常不应发生）。
3. **只把全部源文件都成功处理的条目**收集为 `deletable`；失败的保留在索引里。
4. `db::delete_assets(&conn, &deletable_ids)`。
5. 对每个被删条目的 `thumb_path` / `preview_path`：在删除后查引用计数，为 0 则 `std::fs::remove_file`（**不要**走回收站，缓存丢了可再生）。
6. `app.emit("delete://progress", ...)`，返回 `DeleteSummary`。

> 删除**不**碰设备（设计非目标：不做双向删除）。

- [ ] **Step 4: 前端动作与确认**

`library.ts`：`deleteSelected()` 调 `delete_assets`，成功后 `reload()`。
`App.vue`：选中工具栏加「删除选中」。点击时用 `window.confirm`（或自绘弹层）确认，文案必须包含：
- 「将删除 N 个条目并移入回收站（其中实况条目会同时删除静态图与视频）」；
- 「回收站容量不足时，大文件可能被永久删除」；
- 取消则不做。

- [ ] **Step 5: 测试与构建**

Run: `cargo test -p liveporter`; `npx vue-tsc --noEmit; npm run build`
Expected: 通过。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/delete.rs src-tauri/src/db.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/stores/library.ts src/App.vue
git commit -m "feat(app): delete assets to recycle bin with cache cleanup"
```

---

## Task 9: 实测与收尾

**Files:** Create `docs/superpowers/notes/2026-09-18-browse-smoke.md`

- [ ] **Step 1: 造一个多日期合成库**（或在既有库上）

至少覆盖：跨多天的 `taken_at`、实况/照片/视频各若干、以及一条 `taken_at=0`。

- [ ] **Step 2: 逐项实测**

| 检查项 | 期望 |
|---|---|
| 时间线 | 按天/月/年分段、日期头正确、滚动时头与瓦片一起虚拟化 |
| 粒度切换 | 缩放跨过阈值时自动换段，且视图不跳；手动「年/月/天」覆盖生效 |
| 分页 | 滚动到底自动加载下一页；加载中不重复请求；总数正确 |
| 搜索 | 输入文件名子串生效；快速输入只发一次请求 |
| 类型/日期/完整性筛选 | 结果与 SQL 预期一致 |
| 缩放锚点 | Ctrl+滚轮后光标下的条目不跳（含跨日期头时） |
| 悬停预览 | 与阶段 5 行为一致，未因重写退化 |
| 多选/全选 | 计数正确；全选大集不卡 UI |
| 导出 | 配对文件一起拷出；重名时两个文件后缀一致；字节数与源一致（`Get-FileHash` 抽查） |
| 删除 | 原文件进回收站（可从回收站还原）；索引行消失；缓存仅在无引用时删除 |
| 删除后刷新 | 网格/统计正确更新 |

- [ ] **Step 3: 记录并提交**

```bash
git add docs/superpowers/notes/2026-09-18-browse-smoke.md
git commit -m "docs: record browse UI smoke test"
```

- [ ] **Step 4: 全量验证**

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test -p probe -p liveporter
cargo clippy -p probe -p liveporter --all-targets -- -D warnings
cargo fmt --check
npx vue-tsc --noEmit
npm run build
```

- [ ] **Step 5: 更新 `AGENTS.md`**：把计划 8 标为完成、下一步改为计划 9（打包分发），并把本阶段的「需过审确认」结论固化进设计文档（若确认）。

---

## 完成判据

- [ ] 查询按 `taken_at DESC` 排序，`AssetRow` 带 `taken_at`
- [ ] `AssetFilter` 支持文字/类型/完整性/日期区间（有单测，含 LIKE 转义）
- [ ] 重扫不再产生重复行（有单测）
- [ ] 时间线支持**年/月/天**多级分段（随缩放自动切换，且可手动指定），带日期头，且仍走虚拟滚动
- [ ] 分页增量加载（不再硬编码 5000）
- [ ] 多选 + 全选（全选走 `list_asset_ids`，整库范围）
- [ ] 导出原样拷贝配对文件；重名后缀成对一致（有单测）
- [ ] 删除走 Windows 回收站；索引与孤儿缓存同步清理（缓存引用计数有单测）
- [ ] `cargo test / clippy -D warnings / fmt --check`、`vue-tsc`、`npm run build` 全绿
- [ ] **未做** 打包分发与 ffmpeg 裁剪（计划 9）

## 明确不在本计划内

- 首次引导、iCloud 自动检测、打包分发、ffmpeg 裁剪 → 计划 9（设计 §11、§13 第 9 步）
- 单张「大图查看」按需解码（设计 §7.1 大图）**不属于 §9 的四项**，列为后续
- 时间线的「年/月」多级缩放（iOS 式）；本计划只做「按天」一级
- 双向删除（同步删除手机文件）——设计非目标

---

## 已确认决策（2026-09-18 过审）

| # | 议题 | 决策 |
|---|---|---|
| 1 | 时间线粒度 | **年/月/天多级**：默认随缩放列数自动切换（≤5 天 / 6~9 月 / ≥10 年），另给「自动/年/月/天」手动覆盖 |
| 2 | 「全选」范围 | **当前筛选条件下的整库**（`list_asset_ids` 返回全部匹配 id） |
| 3 | 删除单位 | **逻辑条目成对删**（实况的静态图与视频一起进回收站），确认文案说明 |
| 4 | 回收站实现 | **用 `trash = "5"`** |
| 5 | 导出目录结构 | **平铺拷贝**到所选目录，重名成对加后缀 |
| 6 | 历史重复行 | **本计划只防新增**，历史重复仍靠清库重导 |
| 7 | `taken_at=0` | **归入「未知日期」桶** |
| 8 | 文本搜索 | **`base_name` LIKE 子串**（含转义），不引入 FTS |
| 9 | 导出/删除取消 | **共用现有取消标志**（`cancel_import`） |

## 剩余风险与注意

1. **多级粒度是「换分段标题」，不是 iOS 那种年/月缩略图拼贴页**；后者属后续增强。
2. **「全选」id 数组体积**（十万级）需在 Task 9 实测记录。
3. **重扫 `taken_at` 修复只阻止新增重复，不清理历史重复**（`notes/2026-09-18-integrity-smoke.md` 已记录）。
4. **缓存删除靠引用计数**，务必先删行、再按剩余引用判断，避免误删共享缩略图/预览片。
5. **`LIKE '%…%'` 不走索引**：十万级可接受，更大规模需 FTS（本计划不做）。
6. **导出/删除与导入共用取消标志**，UI 上按钮需互斥，同一时刻只跑一个长任务。
7. **`trash = "5"` 对产物体积/编译时间的影响**待 Task 0 记录。

## 自查记录

**规格覆盖：** 设计 §9（时间线、搜索筛选、多选导出、从库删除）、§10.3（回收站语义与容量提示）、§5.3（重名后缀与配对）、§7.2（缓存复用 → 删除需引用计数）、§8.3（前端不持全量、分页）。

**对既有实现的承接：** 会删除 `db::page_assets`（唯一调用点是 `commands::list_assets`）并改其排序；会重写 `PhotoGrid.vue` 的渲染层（保留占位图/滚动停稳再加载/锚点缩放的既有行为）；`AssetRow` 加字段属向后兼容。

**已知不确定点（实现时必须查证）：** 见文首「待实测 1~4」。
