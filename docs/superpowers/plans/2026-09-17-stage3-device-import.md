# 阶段 3：设备导入（WPD 传输 + 状态机 + 断点续传）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 iPhone 上的实况照片/照片/视频**只读地**拷进计划 3 建立的库：增量比对、流式传输、状态机、断点续传、进度可见。这是 LivePorter 第一次真正读取设备上的文件字节。

**Architecture:** 把阶段 0 探测程序里验证过的 WPD 枚举逻辑**移植**进 `src-tauri`（探针 `crates/probe` 保持不动，它是一次性程序），再加上传输层。管道分两段：**探测/比对**（只读元数据）产出任务清单；**传输**（读字节写盘 + 更新索引）。两者共用同一个 `DeviceSource`，纯逻辑部分可脱离设备单测。

**Tech Stack:** Rust 2021 / `windows` 0.58（WPD COM + `IStream`）/ `rusqlite`（计划 3 已有）/ Tauri 2（事件上报进度）

**为什么这一步风险高（设计 §12）：** WPD 传输速度由设备端决定（经验 5~15 MB/s）；并发流数量需实测；中途拔线/断电不能留下不一致状态。本计划用「状态机 + 断点续传」把风险收敛，并**必须先做小规模实测**（Task 8）再谈整机导入。

---

## 关键事实（2026-09-17 查证 `windows` 0.58 源码，勿凭记忆改）

1. **取文件内容流的接口（原文签名）：**
   ```rust
   // IPortableDeviceContent
   pub unsafe fn Transfer(&self) -> Result<IPortableDeviceResources>
   // IPortableDeviceResources
   pub unsafe fn GetStream<P0>(
       &self,
       pszobjectid: P0,                                   // 对象 ID
       key: *const PROPERTYKEY,                           // WPD_RESOURCE_DEFAULT
       dwmode: u32,                                       // STGM_READ.0 (=0)
       pdwoptimalbuffersize: *mut u32,                    // 出参：建议缓冲大小
       ppstream: *mut Option<IStream>,                    // 出参：流
   ) -> Result<()>
   ```
   `WPD_RESOURCE_DEFAULT` 已由 crate 导出；`STGM_READ` 在 `Win32::System::Com`。

2. **读流（注意返回的是 `HRESULT` 不是 `Result`）：**
   ```rust
   pub unsafe fn Read(&self, pv: *mut c_void, cb: u32, pcbread: Option<*mut u32>) -> HRESULT
   ```
   `cb` 是请求字节数；`pcbread` 回填实际读到的字节数；**读到 0 表示结束**。用 `.is_ok()` 判断（`S_FALSE` 也算 ok）。

3. **`Read` 的返回**：读满时 `S_OK`，读到结尾 `S_FALSE`，都可能带部分数据。循环条件应是「上一次读到 0 字节才停」，而不是「HRESULT 失败才停」。

4. **拍摄时间**：`WPD_OBJECT_DATE_CREATED` 是 `VT_DATE`。`windows-core` 提供 `impl TryFrom<&PROPVARIANT> for f64`（走 `PropVariantToDouble`），得到 OLE Automation 日期（自 1899-12-30 起的天数）。转 Unix epoch：`epoch = (ole - 25569.0) * 86400.0`。

5. **设备标识**：`WPD_DEVICE_MODEL`、`WPD_DEVICE_SERIAL_NUMBER` 通过 `content.Properties().GetValues(WPD_DEVICE_OBJECT_ID, keys)` 读取。设备目录名 = `<model>-<serial 后6位>`（设计 §4）。

6. **只读承诺**：本计划**只**调用 `Transfer`/`GetStream`/`Read`/`*::GetValues`/`EnumObjects`。整个 `src-tauri/src/device/` 目录由只读守卫测试看守（沿用计划 1 的做法，needle 用方法调用形态）。

**待实测 / 未查证（实现时不得凭记忆写）：**

1. `PropVariantToDouble` 对 `VT_DATE` 是否真的返回 OLE 日期（可能失败）——Task 5 必须先打日志实测；失败则回退用 `PropVariantToBSTR` 解析字符串，或改用 `VariantTimeToSystemTime`。
2. `GetStream` 的 `dwmode` 用 `STGM_READ.0` 是否正确——以能否读到数据为准。
3. iOS 上 `WPD_RESOURCE_DEFAULT` 是否对所有媒体对象可用——需实机验证（探针已验证过元数据可读，但没读过字节）。
4. 传输速率与并发流数量的关系（设计附录 B 第 3 项）——Task 8 实测记录。

---

## 文件结构

```
src-tauri/src/
├── device/
│   ├── mod.rs            WPD 设备源：枚举/打开/媒体清单（从 crates/probe 移植并扩展）
│   ├── keys.rs           重导出 WPD_* 常量（与 probe 相同，直接搬）
│   └── transfer.rs       单文件流式下载：GetStream + IStream::Read
├── importer.rs           增量比对 + 传输管道 + 状态机 + 断点续传（纯逻辑 + 通过抽象调用 device）
├── lib.rs                修改：注册命令与事件
├── state.rs              修改：AppState 增加设备源（惰性打开）
└── commands.rs           修改：加设备相关命令

src/
├── stores/import.ts      新增：导入状态与进度
└── App.vue               修改：加导入面板
```

**边界说明：**
- `importer.rs` 的任务清单/状态机是**纯逻辑**，用假 `TransferSink` 单测，不碰设备。
- `device/` 下的所有 COM 调用集中在 `device/mod.rs` 与 `device/transfer.rs`。
- `crates/probe` **不修改**；探测逻辑是「移植」不是「依赖」。

---

## Task 0: 移植 WPD 设备访问层

**Files:**
- Create: `src-tauri/src/device/mod.rs`、`src-tauri/src/device/keys.rs`
- Modify: `src-tauri/src/lib.rs`（加 `mod device;`）
- Modify: `src-tauri/Cargo.toml`（windows crate）

**做法：** 把 `crates/probe/src/wpd/keys.rs` 与 `crates/probe/src/wpd/device.rs` 的逻辑搬进 `src-tauri/src/device/`，并把 `DeviceSource` 的方法扩展到需要的东西：`device_info`、`list_media`（含 `object_id`、`name`、`size`、`date_created`）。**不要**直接依赖 probe crate。

- [ ] **Step 1: 加 windows 依赖（feature 与 probe 一致，另加 `Win32_System_Com`）**

Run:
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo add windows --package liveporter --features Win32_Devices_PortableDevices,Win32_System_Com,Win32_System_Com_StructuredStorage,Win32_UI_Shell_PropertiesSystem,Win32_Foundation
```
把实际版本记在下方：

```
（待填）
```

- [ ] **Step 2: 搬运 `keys.rs`**

直接复制 `crates/probe/src/wpd/keys.rs` 到 `src-tauri/src/device/keys.rs`（重导出 `WPD_*` 常量 + 那个 `property_keys_are_distinct` 测试）。

- [ ] **Step 3: 搬运并精简 `device.rs`**

把 `crates/probe/src/wpd/device.rs` 的核心搬进 `src-tauri/src/device/mod.rs`，**保留**：
- `ComGuard`（或改用 Tauri 自带的 COM 初始化——见 Step 4）
- 按友好名过滤 Apple/iPhone、逐个 `Open`、跳过会挂起的非 Apple 设备（计划 1 §2.5 的教训）
- 递归枚举整卷、按扩展名/内容类型分类
- `RemoteFile { object_id, name, size, date_created }`

**改动：**
- 序列号**不要**打码（库目录名需要后 6 位；打码只在诊断报告里用）。但要保证它不会被打印到日志。
- 增加一个 `MediaFile` 结构，包含 `object_id`（传输时必需）。
- 去掉 probe 的报告逻辑（那是探测专用）。

- [ ] **Step 4: 处理 COM 初始化**

Tauri 主线程已初始化 COM（OLE）。`GetStream`/`Read` 需要 STA。**做法：起一个专用线程跑所有 WPD 操作**（`std::thread::spawn` + 在该线程内 `CoInitializeEx(APARTMENTTHREADED)`），用 channel 与 async 侧通信。把这条约定写进 `device/mod.rs` 顶部注释。**不要**在主线程直接调 WPD（会阻塞 UI）。

- [ ] **Step 5: 编译**

Run:
```powershell
cargo check -p liveporter
```
Expected: 通过。

- [ ] **Step 6: 加只读守卫测试**

在 `lib.rs` 的测试模块（或新建 `src-tauri/src/device/mod.rs` 里的 `#[cfg(test)]`）加：
```rust
#[cfg(test)]
mod readonly_guard {
    #[test]
    fn device_module_contains_no_write_operations() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/device");
        let mut offenders = Vec::new();
        for entry in walkdir(dir) {
            let text = std::fs::read_to_string(&entry).unwrap();
            for needle in [".Delete(", ".Move(", ".Copy(", "CreateObject", "CreateResource", "CopyHere"] {
                if text.contains(needle) {
                    offenders.push(format!("{entry}: {needle}"));
                }
            }
        }
        assert!(offenders.is_empty(), "设备访问必须是只读的: {offenders:?}");
    }

    fn walkdir(dir: &str) -> Vec<String> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walkdir(p.to_str().unwrap()));
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p.to_str().unwrap().to_string());
            }
        }
        out
    }
}
```

- [ ] **Step 7: 提交**

```bash
git add src-tauri/Cargo.toml Cargo.lock src-tauri/src/device src-tauri/src/lib.rs
git commit -m "feat(app): port read-only WPD device access from probe"
```

---

## Task 1: 单文件流式下载 `device/transfer.rs`

**Files:** Create `src-tauri/src/device/transfer.rs`

- [ ] **Step 1: 实现下载函数**

```rust
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use windows::core::PCWSTR;
use windows::Win32::Devices::PortableDevices::{
    IPortableDeviceContent, WPD_RESOURCE_DEFAULT,
};
use windows::Win32::System::Com::{IStream, STGM_READ};

/// 把设备上的一个对象流式写到本地文件。
///
/// 目标路径若已存在会被**覆盖**（调用方负责保证路径正确/已加后缀）。
/// `on_progress` 每读完一块回调一次，用于上报进度。
pub fn download_object(
    content: &IPortableDeviceContent,
    object_id: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64),
) -> Result<u64> {
    let wide: Vec<u16> = object_id.encode_utf16().chain(std::iter::once(0)).collect();

    let resources = content.Transfer().context("获取 Transfer 接口失败")?;

    let mut buf_size: u32 = 0;
    let mut stream: Option<IStream> = None;
    unsafe {
        resources
            .GetStream(
                PCWSTR(wide.as_ptr()),
                &WPD_RESOURCE_DEFAULT,
                STGM_READ.0,
                &mut buf_size,
                &mut stream,
            )
            .context("GetStream 失败")?;
    }
    let stream = stream.context("设备未返回数据流")?;

    if buf_size == 0 {
        buf_size = 64 * 1024;
    }
    let chunk = buf_size.min(1024 * 1024) as usize;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(dest).with_context(|| format!("创建 {dest:?} 失败"))?;
    let mut writer = BufWriter::with_capacity(chunk, file);

    let mut buf = vec![0u8; chunk];
    let mut total: u64 = 0;
    loop {
        let mut read: u32 = 0;
        let hr = unsafe {
            stream.Read(
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                chunk as u32,
                Some(&mut read),
            )
        };
        if hr.is_err() {
            bail!("IStream::Read 失败: {hr:?}");
        }
        if read == 0 {
            break;
        }
        writer.write_all(&buf[..read as usize])?;
        total += read as u64;
        on_progress(total);
    }
    writer.flush()?;

    unsafe {
        let _ = resources.Cancel();
    }
    Ok(total)
}
```

> `STGM_READ.0`：`STGM` 是 newtype，取内部 u32。以编译为准。
> `IStream` 是 `windows::Win32::System::Com::IStream`，需要 `Win32_System_Com` feature。

- [ ] **Step 2: 加取消支持（可选但推荐）**

加一个 `download_object_cancellable(content, object_id, dest, is_cancelled: impl Fn() -> bool, on_progress)`：在循环里检查 `is_cancelled()`，为真则 `resources.Cancel()` 并返回 `Err`。**实现时二选一**，别两个都留。

- [ ] **Step 3: 编译**

Run:
```powershell
cargo check -p liveporter
```
Expected: 通过。若 `IStream::Read` 的参数类型不符，以编译错误为准调整（见「关键事实 2」）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/device/transfer.rs src-tauri/src/device/mod.rs
git commit -m "feat(app): stream a device object to disk"
```

---

## Task 2: 目标路径与命名（纯逻辑）

**Files:** Create `src-tauri/src/importer.rs`

**规则（设计 §4 与 §5.3）：**
- 目录：`originals/<device_folder>/<year>/`
- `year` 来自 `taken_at`（拍摄时间）的本地年份；取不到时间时用 `unknown`。
- 文件名沿用设备上的原始文件名（含扩展名）。
- 撞车（目标已存在且不是同一设备对象）→ 加后缀 `_1`、`_2`，且**配对的两个文件必须同步改名**（设计 §5.3）。

- [ ] **Step 1: 写失败的测试**

```rust
use std::path::{Path, PathBuf};

/// 目标目录：originals/<device_folder>/<year>/
pub fn asset_dir(originals: &Path, device_folder: &str, year: Option<i32>) -> PathBuf {
    let y = year.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into());
    originals.join(device_folder).join(y)
}

/// 生成不冲突的文件名。`taken` 是本次要写的所有文件名（配对的两个）。
/// 若任一同名文件已存在，则整体加后缀 `_N`，保证配对关系同步改名。
pub fn unique_names(dir: &Path, names: &[String]) -> Vec<String> {
    let collide = |suffix: &str| {
        names.iter().any(|n| {
            let candidate = add_suffix(n, suffix);
            dir.join(candidate).exists()
        })
    };
    if !collide("") {
        return names.to_vec();
    }
    let mut i = 1;
    loop {
        let suffix = format!("_{i}");
        if !collide(&suffix) {
            return names.iter().map(|n| add_suffix(n, &suffix)).collect();
        }
        i += 1;
    }
}

fn add_suffix(name: &str, suffix: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{}{}{}", &name[..i], suffix, &name[i..]),
        _ => format!("{name}{suffix}"),
    }
}
```

测试：
```rust
#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn year_dir_uses_unknown_when_missing() {
        let root = Path::new("D:/Lib/originals");
        assert_eq!(
            asset_dir(root, "iPhone15Pro-3F9A2C", Some(2024)),
            root.join("iPhone15Pro-3F9A2C").join("2024")
        );
        assert_eq!(
            asset_dir(root, "iPhone15Pro-3F9A2C", None),
            root.join("iPhone15Pro-3F9A2C").join("unknown")
        );
    }

    #[test]
    fn unique_names_adds_suffix_to_both_files() {
        let dir = std::env::temp_dir().join("lpm_names_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("IMG_0001.HEIC"), b"x").unwrap();

        let names = vec!["IMG_0001.HEIC".to_string(), "IMG_0001.MOV".to_string()];
        let out = unique_names(&dir, &names);
        assert_eq!(out, vec!["IMG_0001_1.HEIC", "IMG_0001_1.MOV"]);
    }
}
```

- [ ] **Step 2: 运行测试**

Run:
```powershell
cargo test -p liveporter path_tests
```
Expected: 2 个通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/importer.rs src-tauri/src/lib.rs
git commit -m "feat(app): plan destination paths and collision-safe names"
```

---

## Task 3: 拍摄时间解析（`taken_at`）

**Files:** Modify `src-tauri/src/device/mod.rs`

- [ ] **Step 1: 先实测 `PropVariantToDouble` 对 `VT_DATE` 的行为**

在 `device/mod.rs` 的枚举里，对每个对象读 `WPD_OBJECT_DATE_CREATED`，打印原始 `PROPVARIANT` 的类型与转换结果到 stderr（临时）：

```rust
let pv = values.GetValue(&WPD_OBJECT_DATE_CREATED).ok();
eprintln!("date vt={:?} as_f64={:?}", pv.as_ref().map(|p| p.as_raw()), pv.as_ref().and_then(|p| f64::try_from(p).ok()));
```

Run（插上 iPhone）:
```powershell
cargo run -p livephoto-manager --release 2>&1 | Select-Object -First 20
```
> 应用是 Tauri，没有命令行版。**更简单：把这段临时逻辑写进一个 `#[test] #[ignore]` 的测试里跑。** 见 Step 2。

- [ ] **Step 2: 决定取时间的手段并实现**

优先：
```rust
/// OLE Automation 日期 → Unix epoch 秒。取不到返回 None。
pub fn taken_at_from_pv(pv: &windows::core::PROPVARIANT) -> Option<i64> {
    let ole = f64::try_from(pv).ok()?;
    if ole <= 0.0 {
        return None;
    }
    Some(((ole - 25569.0) * 86400.0) as i64)
}

/// 由 epoch 秒取本地年份。
pub fn year_of(epoch: i64) -> Option<i32> {
    // 用 time crate 或手算；不引 chrono。
    // 手算 UTC 年份即可（差一天的边界对本用途可接受）。
    let days = epoch.div_euclid(86400);
    // 从 1970-01-01 起推算年份
    let mut year = 1970i64;
    let mut d = days;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if d < len {
            break;
        }
        d -= len;
        year += 1;
    }
    Some(year as i32)
}
```

测试 `year_of`：
```rust
#[test]
fn year_of_epoch() {
    assert_eq!(year_of(0), Some(1970));
    assert_eq!(year_of(1_700_000_000), Some(2023));
}
```

> 若 Step 1 实测 `PropVariantToDouble` 失败，则改用 `BSTR::try_from(pv)` 拿到本地化日期串，再用一个小的手写解析（形如 `2024/05/01 12:00:00`）——**这种情况必须把发现写进设计文档 §2.5 之类的实测记录**，不要静默换方案。

- [ ] **Step 3: 把 `taken_at` 接进 `MediaFile`**

枚举时对每个对象填 `taken_at: Option<i64>` 与 `year: Option<i32>`。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/device/mod.rs
git commit -m "feat(app): derive capture time from device metadata"
```

---

## Task 4: 增量比对与任务清单（纯逻辑 + 抽象）

**Files:** Modify `src-tauri/src/importer.rs`、`src-tauri/src/db.rs`

**规则（设计 §5.3、§6）：**
- 真重复：业务键 `(device_id, base_name, taken_at)` 命中且大小一致 → **跳过**。
- 新条目 → 加入任务清单。
- 需要「设备上的文件清单」与「索引里的条目」两个输入，输出「要传输的文件」清单。

- [ ] **Step 1: 写失败的测试**

```rust
use std::collections::HashMap;

use rusqlite::Connection;

/// 设备上的一个待传输文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFile {
    pub object_id: String,
    pub name: String,
    pub size: u64,
    pub taken_at: Option<i64>,
}

/// 一个逻辑条目的任务：静态图与视频（可缺一）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferTask {
    pub base_name: String,
    pub taken_at: i64,
    pub still: Option<DeviceFile>,
    pub movie: Option<DeviceFile>,
}

/// 与索引比对，产出需要传输的条目。
/// `existing`: (base_name, taken_at) -> (still_size, movie_size)（已入库的大小）
pub fn diff_tasks(
    device_files: &[DeviceFile],
    existing: &HashMap<(String, i64), (Option<u64>, Option<u64>)>,
) -> Vec<TransferTask> {
    // 按 (base_name, taken_at) 分组
    let mut groups: std::collections::BTreeMap<(String, i64), TransferTask> = Default::default();
    for f in device_files {
        let base = crate::pairing::base_name(&f.name).to_string();
        let ext = /* 取扩展名，小写 */;
        let taken = f.taken_at.unwrap_or(0);
        let e = groups.entry((base.clone(), taken)).or_insert_with(|| TransferTask {
            base_name: base,
            taken_at: taken,
            still: None,
            movie: None,
        });
        if crate::pairing::is_still_name(&f.name) {
            e.still = Some(f.clone());
        } else if crate::pairing::is_movie_name(&f.name) {
            e.movie = Some(f.clone());
        }
    }

    groups
        .into_values()
        .filter(|t| {
            let key = (t.base_name.clone(), t.taken_at);
            match existing.get(&key) {
                None => true, // 新条目
                Some((still, movie)) => {
                    // 大小不一致才算需要重传
                    let same_still = match (&t.still, still) {
                        (Some(f), Some(s)) => f.size == *s,
                        (None, None) => true,
                        _ => false,
                    };
                    let same_movie = match (&t.movie, movie) {
                        (Some(f), Some(s)) => f.size == *s,
                        (None, None) => true,
                        _ => false,
                    };
                    !(same_still && same_movie)
                }
            }
        })
        .collect()
}
```

> 需要给 `pairing.rs` 加两个公开小函数 `is_still_name` / `is_movie_name`（按文件名判断），供这里复用。实现时补上并加测试。

- [ ] **Step 2: 加 `db::existing_sizes`**

```rust
/// 返回 (base_name, taken_at) -> (still_size, movie_size) 的映射，供增量比对。
pub fn existing_sizes(conn: &Connection) -> rusqlite::Result<
    std::collections::HashMap<(String, i64), (Option<u64>, Option<u64>)>,
> {
    let mut stmt = conn.prepare(
        "SELECT base_name, taken_at, still_size, movie_size FROM asset",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            (r.get::<_, String>(0)?, r.get::<_, i64>(1)?),
            (
                r.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                r.get::<_, Option<i64>>(3)?.map(|v| v as u64),
            ),
        ))
    })?;
    rows.collect()
}
```

- [ ] **Step 3: 运行测试（补一个「重复条目不进任务清单」的测试）**

Run:
```powershell
cargo test -p liveporter diff
```
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/importer.rs src-tauri/src/db.rs src-tauri/src/pairing.rs
git commit -m "feat(app): compute incremental transfer tasks"
```

---

## Task 5: 传输管道 + 状态机 + 断点续传

**Files:** Modify `src-tauri/src/importer.rs`、`src-tauri/src/db.rs`

**状态机（设计 §6.1）：** `pending → copied → done`；失败标 `failed` 并写 `error`。
本计划**没有 ffmpeg**，所以传输成功即 `copied`；`transcoded` 留待计划 5，`done` 暂不产生。

- [ ] **Step 1: 状态常量与事件**

```rust
pub const STATUS_PENDING: i64 = 0;
pub const STATUS_COPIED: i64 = 1;
pub const STATUS_FAILED: i64 = 4;

/// 导入进度事件（通过 Tauri emit 给前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportProgress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub current: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
}
```

- [ ] **Step 2: 加 `db::mark_status`**

```rust
pub fn set_asset_status(
    conn: &Connection,
    device_id: i64,
    base_name: &str,
    taken_at: i64,
    status: i64,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE asset SET status=?1, error=?2, updated_at=?3
         WHERE device_id=?4 AND base_name=?5 AND taken_at=?6",
        params![status, error, now_epoch(), device_id, base_name, taken_at],
    )?;
    Ok(())
}
```

- [ ] **Step 3: 写失败的测试（用假 sink，不碰设备）**

在 `importer.rs`：
```rust
/// 传输动作的抽象：真实实现走 WPD，测试用假的。
pub trait Transfer {
    /// 下载一个设备对象到 dest。
    fn fetch(&mut self, object_id: &str, dest: &Path) -> anyhow::Result<u64>;
}

/// 执行一批任务：每个任务先写索引（status=pending），再传输两个文件，
/// 成功后 status=copied。单个条目失败不阻断其余。
pub fn run_tasks(
    tasks: &[TransferTask],
    originals: &Path,
    device_folder: &str,
    device_id: i64,
    conn: &Connection,
    transfer: &mut dyn Transfer,
    mut on_progress: impl FnMut(&ImportProgress),
) -> anyhow::Result<ImportProgress> { /* 实现 */ }
```

测试用假 `Transfer`：在内存里记录被调用的 object_id，并往 dest 写几个字节；断言：
- 每个任务的两个文件都被 fetch；
- 成功后 DB 里该条目 `status=1`；
- 第二个假 Transfer 若对某 object 返回 Err，该条目 `status=4` 且 `error` 非空，其余条目仍为 1。

- [ ] **Step 4: 实现 `run_tasks`（顺序、默认并发 1）**

逻辑：
1. 对每个 task：`upsert_asset(..., status=pending)`；
2. 计算目标目录与不冲突文件名（Task 2 的函数）；
3. `still` 与 `movie` 分别 `transfer.fetch(object_id, dest)`；
4. 全成功 → `set_asset_status(copied)`；任一失败 → `set_asset_status(failed, err)`；
5. 每完成一个 task 回调一次 `on_progress`。

> **并发**：设计 §6.3 说并发流数量需实测、**默认 1**。本计划就按 1 实现；把「并发度」做成函数参数但只传 1，并在 Task 8 实测后再考虑是否加。

- [ ] **Step 5: 运行测试**

Run:
```powershell
cargo test -p liveporter importer
```
Expected: 通过。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/importer.rs src-tauri/src/db.rs
git commit -m "feat(app): transfer pipeline with status machine and resume"
```

---

## Task 6: 命令与进度事件

**Files:** Modify `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/state.rs`

- [ ] **Step 1: 命令**

```rust
#[tauri::command]
pub async fn import_from_device(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<crate::importer::ImportProgress, String> {
    // WPD 操作在专用线程；进度用 app.emit("import://progress", ...) 上报。
    // 未插入设备时返回明确错误："未发现 iPhone 设备…"
}
```
需要用 `tauri::Emitter` trait 发事件。

- [ ] **Step 2: 注册命令与模块**

`generate_handler!` 加 `import_from_device`；`lib.rs` 加 `mod importer;`。

- [ ] **Step 3: 编译**

Run:
```powershell
cargo check -p liveporter
```
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/state.rs
git commit -m "feat(app): expose device import command with progress events"
```

---

## Task 7: 前端导入面板

**Files:** Create `src/stores/import.ts`；Modify `src/App.vue`

- [ ] **Step 1: store**

```ts
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
}

export const useImport = defineStore("import", () => {
  const running = ref(false);
  const progress = ref<ImportProgress | null>(null);
  const error = ref<string | null>(null);

  async function start() {
    running.value = true;
    error.value = null;
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

  return { running, progress, error, start };
});
```

- [ ] **Step 2: App.vue 加按钮与进度条**

在 header 加「从 iPhone 导入」按钮（需先打开库），下方显示 `done/total` 与当前文件名、已传字节数。**不做样式打磨**（属后续计划）。

- [ ] **Step 3: 类型检查与构建**

Run:
```powershell
npm run build
```
Expected: 通过。

- [ ] **Step 4: 提交**

```bash
git add src/stores/import.ts src/App.vue
git commit -m "feat(ui): device import panel with progress"
```

---

## Task 8: 实机实测（关键，别跳过）

**Files:** Create `docs/superpowers/notes/2026-09-17-import-smoke.md`

- [ ] **Step 1: 小规模试导入**

插上 iPhone，打开一个**空库**，先只在设备上选/确认少量照片（或用一个小库做全量），点「从 iPhone 导入」。观察：
- 是否真的写出了文件（`originals/<device>/<year>/`）。
- 配对的两个文件是否同目录、同主名。
- 文件大小是否与设备报告一致（对比探针报告）。
- 导入完成后重建索引，统计是否与导入数一致。
- 再次导入 → 是否**跳过**已导入的（增量为 0）。

- [ ] **Step 2: 记录传输速率**

用任务管理器或日志记录：导入 N 个文件用了多久、平均 MB/s。这是设计附录 B 第 3 项要的数据。

- [ ] **Step 3: 断点续传验证**

导入中途**拔线**（或强杀进程）：
- 重新插上、重新打开库、再导入 → 已完成的不重传、未完成的续传。
- DB 里 `status` 是否符合预期（已完成的为 1，中断的停在 0）。

- [ ] **Step 4: 记录到 notes 文件并更新设计文档 §12/附录 B**

- [ ] **Step 5: 提交**

```bash
git add docs/superpowers/notes/2026-09-17-import-smoke.md docs/superpowers/specs
git commit -m "docs: record device import smoke test and transfer rate"
```

---

## Task 9: 收尾验证

- [ ] **Step 1: Rust 侧**

Run:
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test -p probe -p liveporter
cargo clippy -p probe -p liveporter --all-targets -- -D warnings
cargo fmt --check
```
Expected: 全绿。

- [ ] **Step 2: 前端侧**

Run:
```powershell
npx vue-tsc --noEmit
npm run build
```
Expected: 通过。

- [ ] **Step 3: 更新 AGENTS.md 进度与文档索引**

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "chore: stage 3 verification and progress update"
```

---

## 完成判据

- [ ] 设备枚举仍能按友好名过滤出 iPhone（不打开会挂起的非 Apple 设备）
- [ ] 单文件下载能写出与设备大小一致的文件
- [ ] 增量比对正确：已导入的条目不再进入任务清单
- [ ] 状态机正确：成功 `status=1`、失败 `status=4` 且带 `error`（有单测，用假 Transfer）
- [ ] 断点续传：中断后重跑只补未完成的
- [ ] 实机导入成功，且「再次导入」增量为 0
- [ ] 传输速率已记录
- [ ] `cargo test / clippy / fmt`、`vue-tsc`、`npm run build` 全绿
- [ ] **未调用**任何写设备操作（只读守卫测试在看守）
- [ ] **未引入** ffmpeg、UUID 校验（属计划 5）

## 明确不在本计划内

- ffmpeg 缩略图/预览片、`transcoded`/`done` 状态（计划 5）
- ContentIdentifier 校验、integrity 1/2/5、诊断报告（计划 5）
- 并发流数量调优（本计划固定并发 1；数据记入 Task 8，调优留后续）
- iCloud「优化储存空间」检测与阻止（设计 §6.4，检测手段未定）
- 浏览体验与 UI 打磨（后续计划）

---

## 已知风险与取舍（必须如实记录）

1. **iOS 是否对所有媒体对象都提供 `WPD_RESOURCE_DEFAULT` 流**——探针只验证过元数据可读，没读过字节。Task 8 Step 1 是小规模试导入，若发现某些对象读不到流，需要记录并按对象类型处理。
2. **拍摄时间可能取不到**（Task 3）。取不到时年份用 `unknown`，`taken_at=0`，业务键退化——与计划 3 相同。
3. **覆盖策略**：目标文件已存在会被覆盖。若同一对象被重复导入到不同 `taken_at`（时间解析不稳定），可能产生两份。Task 8 要特意验证「再次导入增量为 0」这条。
4. **并发固定 1**：整机导入会很慢（设计经验值 5~15 MB/s）。这是刻意的保守起步，先证明正确性。

## 自查记录

**规格覆盖：** 对应设计 §13 第 4 步、§6（导入管道、状态机、断点续传、并发度）、§5.3（去重/重名）、§4（目录组织）、§12（MTP 相关风险）。

**对计划 3 的承接：** 使用计划 3 的 `Library`、`db::upsert_asset`（需扩展 status 参数）、`pairing`。`taken_at` 不再固定为 0（Task 3），这会改变业务键，属计划 3「已知偏差 2」的修正。

**已知不确定点（实现时必须查证）：**

1. `PropVariantToDouble` 对 `VT_DATE` 的行为（Task 3 Step 1 实测）
2. `GetStream` 的 `dwmode` 与 `IStream::Read` 的确切参数（Task 1 Step 3 以编译为准）
3. Tauri 事件发送的 API（`tauri::Emitter` trait 的 `emit` 签名）
4. 计划 3 的 `upsert_asset` 目前不接受 `status` 参数——Task 5 需要扩展它，注意别破坏计划 3 的测试
5. 专用 STA 线程与 Tauri async 运行时的通信方式（Task 0 Step 4）
