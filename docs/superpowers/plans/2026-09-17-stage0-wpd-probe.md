# 阶段 0：WPD 只读探测程序 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用只读方式枚举 iPhone 相册，确认实况照片的 `.MOV` 动态部分是否通过 Windows WPD/MTP 暴露，并验证「按主名配对」能否成立。这是整个项目唯一的 go/no-go 闸门。

**Architecture:** 一个独立的 Rust CLI 二进制。纯逻辑（配对、报告生成）与 WPD 设备访问严格分离：设备访问收敛在一个 `DeviceSource` trait 后面，真实实现走 WPD COM；配对和报告是纯函数，可用假数据完整单元测试。这样即使用户手上没有 iPhone，CI 也能验证大部分代码。

**Tech Stack:** Rust 2021 edition / `windows` crate（Win32 WPD COM 绑定）/ `serde` + `serde_json` / `anyhow`

**为什么必须先做这个：** 设计文档 §12 把"MTP 不暴露 `.MOV`"列为**致命风险**。若不成立，整个技术路线要改成解析本地 iTunes 备份或走 iCloud API。在此之前不应写任何生产代码。

---

## 环境现状（2026-09-17 实测）

| 项 | 状态 | 计划中的处理 |
|---|---|---|
| Rust 工具链 | 未安装 | Task 1 安装 |
| MSVC 编译器 | 已装（VS 2026，`F:\vs2019`，含 cl.exe / link.exe） | 无需处理 |
| Windows SDK | 未安装（无头文件、无 import lib） | Task 2 安装 |
| WebView2 运行时 | 已装 153.0.4234.32 | 本计划不需要 |
| 网络 | static.rust-lang.org / crates.io 均 200 | 无需镜像 |
| 系统 | Windows 10 Pro 19045 | 无需处理 |

---

## 文件结构

```
livephoto-manager/
├── Cargo.toml                      workspace 根
├── .gitignore                      （已存在，需补 target/）
└── crates/
    └── probe/
        ├── Cargo.toml
        └── src/
            ├── main.rs             CLI 入口：参数解析、串联各模块、打印报告
            ├── model.rs            领域类型：DeviceInfo / StorageInfo / RemoteFile / FileKind
            ├── pairing.rs          纯逻辑：按主名配对，分类成对/仅静态/仅视频
            ├── report.rs           纯逻辑：生成人类可读报告 + JSON 报告
            ├── source.rs           DeviceSource trait 定义（设备访问的抽象边界）
            └── wpd/
                ├── mod.rs          真实 WPD 实现，实现 DeviceSource
                ├── keys.rs         WPD PROPERTYKEY 常量（照 PortableDevice.h 抄录）
                └── device.rs       设备枚举、存储枚举、DCIM 遍历、元数据读取
```

**边界说明：**
- `model.rs` / `pairing.rs` / `report.rs` **不含任何 Win32 调用**，可跨平台编译、可在无设备环境下跑单元测试。
- `source.rs` 只定义 trait，是纯逻辑与设备访问之间唯一的接缝。
- `wpd/` 下所有内容在非 Windows 平台通过 `#[cfg(windows)]` 排除。

---

## Task 0: 五分钟前置验证（一次性脚本，不进仓库）

**目的：** 在安装任何工具链之前，先用零成本手段回答 go/no-go。若答案为否，本计划作废。

**Files:**
- 临时创建：`C:\Users\R4ven\AppData\Local\Temp\opencode\probe-live\probe.ps1`（**不要提交进仓库**）

- [ ] **Step 1: 写探测脚本**

```powershell
$shell = New-Object -ComObject Shell.Application
$pc = $shell.NameSpace(17)   # ssfDRIVES = 这台电脑

$phone = $pc.Items() | Where-Object { $_.Name -match 'iPhone|Apple' } | Select-Object -First 1
if (-not $phone) { Write-Output '未找到设备：请确认手机已插上、已解锁、并点了"信任此电脑"'; exit 1 }

Write-Output "设备: $($phone.Name)"

$storage = $phone.GetFolder.Items() | Where-Object { $_.Name -match 'Internal Storage|内部存储' } | Select-Object -First 1
if (-not $storage) { Write-Output '未找到 Internal Storage'; exit 1 }

$dcim = $storage.GetFolder.Items() | Where-Object { $_.Name -eq 'DCIM' } | Select-Object -First 1
if (-not $dcim) { Write-Output '未找到 DCIM（相册可能为空，或需在手机上重新信任）'; exit 1 }

$all = @()
foreach ($sub in $dcim.GetFolder.Items()) {
  if ($sub.IsFolder) { $all += $sub.GetFolder.Items() }
}
Write-Output "DCIM 下文件总数: $($all.Count)"

$byExt = $all | Group-Object { [System.IO.Path]::GetExtension($_.Name).ToLower() } |
         Sort-Object Count -Descending
$byExt | ForEach-Object { Write-Output ("  {0,-8} {1}" -f $_.Name, $_.Count) }

# 关键判定：能否找到同主名的 HEIC + MOV
$stems = @{}
foreach ($f in $all) {
  $stem = [System.IO.Path]::GetFileNameWithoutExtension($f.Name)
  if (-not $stems.ContainsKey($stem)) { $stems[$stem] = @() }
  $stems[$stem] += [System.IO.Path]::GetExtension($f.Name).ToLower()
}
$paired = ($stems.GetEnumerator() | Where-Object {
  $_.Value -contains '.heic' -and $_.Value -contains '.mov'
}).Count
$movOnly = ($stems.GetEnumerator() | Where-Object {
  $_.Value -contains '.mov' -and $_.Value -notcontains '.heic' -and $_.Value -notcontains '.jpg'
}).Count

Write-Output ''
Write-Output "== 结论 =="
Write-Output "同名 HEIC+MOV 配对成功: $paired"
Write-Output "孤立 MOV（疑似残缺实况）: $movOnly"
```

- [ ] **Step 2: 手机插上、解锁、点"信任此电脑"，然后运行**

Run:
```powershell
pwsh -File "C:\Users\R4ven\AppData\Local\Temp\opencode\probe-live\probe.ps1"
```

Expected: 打印出设备名、DCIM 文件总数、按扩展名的分类统计、配对成功数。

- [ ] **Step 3: 判定并记录**

- **若「同名 HEIC+MOV 配对成功 > 0」** → **通道可行，继续 Task 1**。同时记录该数字，它是后续增量比对的基准。
- **若 `.MOV` 完全没出现**（扩展名统计里没有 `.mov`）→ **方案作废**。停止本计划，回到设计文档 §12，改评估「解析本地备份」或「iCloud 私有 API」两条替代通道。
- **若出现 `.MOV` 但配对数为 0** → 说明命名规则不是同主名。把前 20 个文件名抄下来，需要重新设计配对策略（可能要用 `ContentIdentifier` 或 `WPD_OBJECT_PERSISTENT_UNIQUE_ID`）。

- [ ] **Step 4: 把结论写进设计文档**

在 `docs/superpowers/specs/2026-09-17-liveporter-design.md` 的 §2.3 表格里，把第一行「MTP 是否稳定暴露 `.MOV`」的验证方式更新为实测结论。

```bash
git add docs/superpowers/specs/2026-09-17-liveporter-design.md
git commit -m "docs: record stage-0 MTP probe result"
```

---

## Task 1: 安装 Rust 工具链

**Files:** 无（环境变更）

- [ ] **Step 1: 用 rustup 安装**

Run:
```powershell
winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements
```

若 winget 不可用，改为下载官方安装器：
```powershell
Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile "$env:TEMP\rustup-init.exe"
& "$env:TEMP\rustup-init.exe" -y --default-toolchain stable-x86_64-pc-windows-msvc --profile minimal
```

- [ ] **Step 2: 重开终端并验证**

新开一个 pwsh 窗口（PATH 变更需要新进程），然后 Run:
```powershell
rustc --version
cargo --version
rustup show
```

Expected: `rustc 1.xx.x`、`cargo 1.xx.x`，`rustup show` 显示 default toolchain 为 `stable-x86_64-pc-windows-msvc`。

- [ ] **Step 3: 验证 MSVC 链接器能被自动找到**

Run:
```powershell
cd $env:TEMP
cargo new linkcheck --bin
cd linkcheck
cargo build
```

Expected: 编译成功，`target\debug\linkcheck.exe` 存在。

**若报错 `link.exe not found`**：说明 Rust 没能自动定位 MSVC。此时需设置环境变量指向已安装的 Visual Studio：
```powershell
$env:VSINSTALLDIR = "F:\vs2019"
```
或安装 VS 的「C++ 生成工具」工作负载后重开终端。

---

## Task 2: 安装 Windows SDK

**为什么需要：** WPD 的 `PROPERTYKEY` 常量定义在 `PortableDevice.h` 里。没有 SDK 就没有权威来源，只能凭记忆写 GUID——这是不可接受的。装 SDK 是为了拿到头文件当唯一真相。

**Files:** 无（环境变更）

- [ ] **Step 1: 用 VS Installer 加装 SDK**

Run（`--passive` 会弹进度窗，不阻塞交互）:
```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vs_installer.exe" modify `
  --installPath "F:\vs2019" `
  --add Microsoft.VisualStudio.Component.Windows11SDK.26100 `
  --passive --norestart
```

若上面的组件 ID 不适用于 VS 2026，先查询可用组件：
```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property catalog_productDisplayVersion
```
然后用 VS Installer 图形界面勾选「Windows 11 SDK」（或任意版本的「Windows 10 SDK」）。

- [ ] **Step 2: 验证头文件到位**

Run:
```powershell
Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\Include" | ForEach-Object { $_.Name }
Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\Include\*\um\PortableDevice.h" | ForEach-Object { $_.FullName }
```

Expected: 列出 SDK 版本号目录，并找到至少一个 `PortableDevice.h`。

- [ ] **Step 3: 把路径记下来备用**

把 `PortableDevice.h` 的完整路径记在下方，Task 7 要用：

```
（待填：C:\Program Files (x86)\Windows Kits\10\Include\<版本>\um\PortableDevice.h）
```

---

## Task 3: 建立 Cargo workspace 与 probe crate 骨架

**Files:**
- Create: `Cargo.toml`
- Create: `crates/probe/Cargo.toml`
- Create: `crates/probe/src/main.rs`
- Modify: `.gitignore`

- [ ] **Step 1: 创建 workspace 根**

Create `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/probe"]

[workspace.package]
edition = "2021"
version = "0.1.0"
# license 待定（设计文档附录 A 决策 6）。定下来之前先不填，
# 填错会让 `cargo publish` 等工具产生错误的元数据。
# license = ""

[workspace.dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
lto = true
codegen-units = 1
```

- [ ] **Step 2: 创建 probe crate 清单**

Create `crates/probe/Cargo.toml`:

```toml
[package]
name = "probe"
edition.workspace = true
version.workspace = true
# license 待定，见 workspace 根说明

[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Devices_PortableDevices",
    "Win32_System_Com",
    "Win32_System_Com_StructuredStorage",
    "Win32_Foundation",
] }
```

> **注意：** `windows` crate 的版本号需在实现时以 `cargo add windows --features Win32_Devices_PortableDevices` 实际写入的为准。若 `Win32_Devices_PortableDevices` 这个 feature 名不存在，用 `cargo doc` 或 crates.io 的 feature 列表确认正确名称后再继续——**不要靠猜**。

- [ ] **Step 3: 写最小 main.rs**

Create `crates/probe/src/main.rs`:

```rust
fn main() {
    println!("probe: stage-0 WPD read-only probe");
}
```

- [ ] **Step 4: 编译并运行**

Run:
```powershell
cargo run -p probe
```

Expected: 打印 `probe: stage-0 WPD read-only probe`，退出码 0。

- [ ] **Step 5: 补 .gitignore**

Modify `.gitignore`，在 `# Rust 构建产物` 段下确认存在：

```
target/
```

- [ ] **Step 6: 提交**

```bash
git add Cargo.toml crates/probe/Cargo.toml crates/probe/src/main.rs .gitignore Cargo.lock
git commit -m "chore: bootstrap cargo workspace with probe crate"
```

---

## Task 4: 领域类型 `model.rs`

**Files:**
- Create: `crates/probe/src/model.rs`
- Modify: `crates/probe/src/main.rs`（加 `mod model;`）
- Test: 单元测试写在 `model.rs` 内

- [ ] **Step 1: 写失败的测试**

在 `crates/probe/src/model.rs` 中先只写类型声明和测试：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    Image,
    Video,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub object_id: String,
    pub name: String,
    pub size: u64,
    pub date_created: Option<String>,
    pub kind: FileKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str) -> RemoteFile {
        RemoteFile {
            object_id: "o1".into(),
            name: name.into(),
            size: 1,
            date_created: None,
            kind: FileKind::Image,
        }
    }

    #[test]
    fn base_name_strips_extension() {
        assert_eq!(f("IMG_1234.HEIC").base_name(), "IMG_1234");
    }

    #[test]
    fn base_name_handles_name_without_extension() {
        assert_eq!(f("NOEXT").base_name(), "NOEXT");
    }

    #[test]
    fn base_name_handles_leading_dot() {
        assert_eq!(f(".hidden").base_name(), ".hidden");
    }

    #[test]
    fn extension_is_lowercased() {
        assert_eq!(f("IMG_1234.HEIC").extension(), "heic");
        assert_eq!(f("IMG_1234.MOV").extension(), "mov");
    }

    #[test]
    fn extension_empty_when_absent() {
        assert_eq!(f("NOEXT").extension(), "");
    }
}
```

- [ ] **Step 2: 运行测试，确认编译失败**

Run: `cargo test -p probe`
Expected: FAIL —— `no method named 'base_name' found for struct 'RemoteFile'`

- [ ] **Step 3: 实现方法**

在 `model.rs` 的 `impl RemoteFile` 中加入：

```rust
impl RemoteFile {
    /// 主名：去掉最后一个扩展名。`.` 开头且无其他点号的视为无扩展名。
    pub fn base_name(&self) -> &str {
        match self.name.rfind('.') {
            Some(i) if i > 0 => &self.name[..i],
            _ => &self.name,
        }
    }

    /// 扩展名，小写，不含点。无扩展名返回空串。
    pub fn extension(&self) -> String {
        match self.name.rfind('.') {
            Some(i) if i > 0 => self.name[i + 1..].to_ascii_lowercase(),
            _ => String::new(),
        }
    }
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p probe`
Expected: PASS，5 个测试全绿。

- [ ] **Step 5: 提交**

```bash
git add crates/probe/src/model.rs crates/probe/src/main.rs
git commit -m "feat(probe): add RemoteFile domain type with base name extraction"
```

---

## Task 5: 配对逻辑 `pairing.rs`

**Files:**
- Create: `crates/probe/src/pairing.rs`
- Modify: `crates/probe/src/main.rs`

- [ ] **Step 1: 写失败的测试**

Create `crates/probe/src/pairing.rs`:

```rust
use crate::model::{FileKind, RemoteFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairStatus {
    /// 静态图 + 视频都有 → 实况照片
    Paired,
    /// 只有静态图，且是 HEIC/JPG
    StillOnly,
    /// 只有视频
    VideoOnly,
}

#[derive(Debug, Clone)]
pub struct Pair {
    pub base_name: String,
    pub still: Option<RemoteFile>,
    pub movie: Option<RemoteFile>,
    pub status: PairStatus,
}

pub fn pair(_files: &[RemoteFile]) -> Vec<Pair> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileKind;

    fn f(name: &str, kind: FileKind) -> RemoteFile {
        RemoteFile {
            object_id: name.into(),
            name: name.into(),
            size: 100,
            date_created: None,
            kind,
        }
    }

    #[test]
    fn pairs_still_with_movie_of_same_base_name() {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
        ];
        let pairs = pair(&files);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].base_name, "IMG_0001");
        assert_eq!(pairs[0].status, PairStatus::Paired);
        assert!(pairs[0].still.is_some());
        assert!(pairs[0].movie.is_some());
    }

    #[test]
    fn classifies_lone_image_as_still_only() {
        let pairs = pair(&[f("IMG_0002.HEIC", FileKind::Image)]);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].status, PairStatus::StillOnly);
        assert!(pairs[0].movie.is_none());
    }

    #[test]
    fn classifies_lone_video_as_video_only() {
        let pairs = pair(&[f("IMG_0003.MOV", FileKind::Video)]);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].status, PairStatus::VideoOnly);
        assert!(pairs[0].still.is_none());
    }

    #[test]
    fn keeps_result_sorted_by_base_name() {
        let files = vec![
            f("IMG_0003.MOV", FileKind::Video),
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
        ];
        let pairs = pair(&files);
        let names: Vec<_> = pairs.iter().map(|p| p.base_name.as_str()).collect();
        assert_eq!(names, vec!["IMG_0001", "IMG_0003"]);
    }

    #[test]
    fn ignores_unrelated_file_types() {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
            f("IMG_0001.AAE", FileKind::Other),
        ];
        let pairs = pair(&files);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].status, PairStatus::Paired);
    }

    #[test]
    fn jpg_still_pairs_too() {
        let files = vec![
            f("IMG_0004.JPG", FileKind::Image),
            f("IMG_0004.MOV", FileKind::Video),
        ];
        let pairs = pair(&files);
        assert_eq!(pairs[0].status, PairStatus::Paired);
    }
}
```

在 `main.rs` 顶部加：

```rust
mod model;
mod pairing;
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p probe pairing`
Expected: FAIL —— `not implemented` panic。

- [ ] **Step 3: 实现配对**

把 `pairing.rs` 里的 `unimplemented!()` 替换为：

```rust
pub fn pair(files: &[RemoteFile]) -> Vec<Pair> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<String, Pair> = BTreeMap::new();

    for file in files {
        if file.kind == FileKind::Other {
            continue;
        }
        let base = file.base_name().to_string();
        let entry = map.entry(base.clone()).or_insert_with(|| Pair {
            base_name: base,
            still: None,
            movie: None,
            status: PairStatus::StillOnly,
        });
        match file.kind {
            FileKind::Image => entry.still = Some(file.clone()),
            FileKind::Video => entry.movie = Some(file.clone()),
            FileKind::Other => {}
        }
    }

    map.into_values()
        .map(|mut p| {
            p.status = match (&p.still, &p.movie) {
                (Some(_), Some(_)) => PairStatus::Paired,
                (Some(_), None) => PairStatus::StillOnly,
                (None, Some(_)) => PairStatus::VideoOnly,
                (None, None) => PairStatus::StillOnly,
            };
            p
        })
        .collect()
}
```

> `BTreeMap` 同时解决了去重（同名只保留一条）和排序（按主名升序），与测试 `keeps_result_sorted_by_base_name` 一致。

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p probe`
Expected: PASS，共 11 个测试全绿。

- [ ] **Step 5: 提交**

```bash
git add crates/probe/src/pairing.rs crates/probe/src/main.rs
git commit -m "feat(probe): pair still and movie by base name"
```

---

## Task 6: 报告生成 `report.rs`

**Files:**
- Create: `crates/probe/src/report.rs`
- Modify: `crates/probe/src/main.rs`

- [ ] **Step 1: 写失败的测试**

Create `crates/probe/src/report.rs`：

```rust
use crate::model::{FileKind, RemoteFile};
use crate::pairing::{Pair, PairStatus};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct ExtensionCount {
    pub extension: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    pub device_name: Option<String>,
    pub device_model: Option<String>,
    pub device_serial_masked: Option<String>,
    pub total_files: usize,
    pub extension_counts: Vec<ExtensionCount>,
    pub paired: usize,
    pub still_only: usize,
    pub video_only: usize,
    /// 判定结论：true 表示 MOV 确实暴露、配对成立
    pub go: bool,
}

pub fn build_report(
    device_name: Option<String>,
    device_model: Option<String>,
    device_serial_masked: Option<String>,
    files: &[RemoteFile],
    pairs: &[Pair],
) -> ProbeReport {
    unimplemented!()
}

pub fn render_human(report: &ProbeReport) -> String {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileKind;

    fn f(name: &str, kind: FileKind) -> RemoteFile {
        RemoteFile {
            object_id: name.into(),
            name: name.into(),
            size: 100,
            date_created: None,
            kind,
        }
    }

    fn sample() -> (Vec<RemoteFile>, Vec<Pair>) {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0001.MOV", FileKind::Video),
            f("IMG_0002.HEIC", FileKind::Image),
            f("IMG_0003.MOV", FileKind::Video),
        ];
        let pairs = crate::pairing::pair(&files);
        (files, pairs)
    }

    #[test]
    fn counts_extensions_case_insensitively() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        let heic = r
            .extension_counts
            .iter()
            .find(|e| e.extension == "heic")
            .unwrap();
        assert_eq!(heic.count, 2);
        let mov = r
            .extension_counts
            .iter()
            .find(|e| e.extension == "mov")
            .unwrap();
        assert_eq!(mov.count, 2);
    }

    #[test]
    fn tallies_pair_statuses() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        assert_eq!(r.paired, 1);
        assert_eq!(r.still_only, 1);
        assert_eq!(r.video_only, 1);
        assert_eq!(r.total_files, 4);
    }

    #[test]
    fn go_is_true_when_any_pair_succeeds() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        assert!(r.go);
    }

    #[test]
    fn go_is_false_when_mov_is_never_exposed() {
        let files = vec![
            f("IMG_0001.HEIC", FileKind::Image),
            f("IMG_0002.HEIC", FileKind::Image),
        ];
        let pairs = crate::pairing::pair(&files);
        let r = build_report(None, None, None, &files, &pairs);
        assert!(!r.go);
    }

    #[test]
    fn human_output_warns_when_no_mov_found() {
        let files = vec![f("IMG_0001.HEIC", FileKind::Image)];
        let pairs = crate::pairing::pair(&files);
        let r = build_report(None, None, None, &files, &pairs);
        let text = render_human(&r);
        assert!(text.contains("未发现任何 .MOV"), "实际输出:\n{text}");
    }

    #[test]
    fn human_output_does_not_warn_when_paired() {
        let (files, pairs) = sample();
        let r = build_report(None, None, None, &files, &pairs);
        let text = render_human(&r);
        assert!(!text.contains("未发现任何 .MOV"), "实际输出:\n{text}");
    }
}
```

在 `main.rs` 顶部加 `mod report;`。

- [ ] **Step 2: 运行测试，确认失败**

Run: `cargo test -p probe report`
Expected: FAIL —— `not implemented` panic。

- [ ] **Step 3: 实现**

替换两个 `unimplemented!()`：

```rust
pub fn build_report(
    device_name: Option<String>,
    device_model: Option<String>,
    device_serial_masked: Option<String>,
    files: &[RemoteFile],
    pairs: &[Pair],
) -> ProbeReport {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in files {
        *counts.entry(file.extension()).or_insert(0) += 1;
    }

    let paired = pairs
        .iter()
        .filter(|p| p.status == PairStatus::Paired)
        .count();
    let still_only = pairs
        .iter()
        .filter(|p| p.status == PairStatus::StillOnly)
        .count();
    let video_only = pairs
        .iter()
        .filter(|p| p.status == PairStatus::VideoOnly)
        .count();

    ProbeReport {
        device_name,
        device_model,
        device_serial_masked,
        total_files: files.len(),
        extension_counts: counts
            .into_iter()
            .map(|(extension, count)| ExtensionCount { extension, count })
            .collect(),
        paired,
        still_only,
        video_only,
        go: paired > 0,
    }
}

pub fn render_human(report: &ProbeReport) -> String {
    let mut out = String::new();
    out.push_str("== LivePorter 阶段 0 探测报告 ==\n\n");
    out.push_str(&format!(
        "设备: {} / 型号: {}\n",
        report.device_name.as_deref().unwrap_or("(未知)"),
        report.device_model.as_deref().unwrap_or("(未知)")
    ));
    out.push_str(&format!(
        "序列号: {}\n\n",
        report.device_serial_masked.as_deref().unwrap_or("(未知)")
    ));
    out.push_str(&format!("文件总数: {}\n", report.total_files));
    out.push_str("按扩展名统计:\n");
    for e in &report.extension_counts {
        out.push_str(&format!("  {:<8} {}\n", e.extension, e.count));
    }
    out.push_str(&format!(
        "\n成对(实况): {}\n仅静态图: {}\n仅视频: {}\n",
        report.paired, report.still_only, report.video_only
    ));
    out.push_str("\n== 结论 ==\n");
    if report.go {
        out.push_str("通道可行：MOV 已暴露且能按主名配对。可以继续。\n");
    } else if report.extension_counts.iter().all(|e| e.extension != "mov") {
        out.push_str("未发现任何 .MOV 文件——实况照片的动态部分未通过 MTP 暴露。\n");
        out.push_str("结论：USB 直连通道不可行，需要改用本地备份解析或 iCloud API。\n");
    } else {
        out.push_str("发现了 .MOV 但配对数为 0——文件命名规则与预期不符。\n");
        out.push_str("下一步：抄录前 20 个文件名，重新设计配对策略。\n");
    }
    out
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run: `cargo test -p probe`
Expected: PASS，共 17 个测试全绿。

- [ ] **Step 5: 提交**

```bash
git add crates/probe/src/report.rs crates/probe/src/main.rs
git commit -m "feat(probe): build and render stage-0 probe report"
```

---

## Task 7: WPD 属性键常量 `wpd/keys.rs`

**为什么单独一个任务：** WPD 的 `PROPERTYKEY` 是一堆 GUID + PID 常量。**这些值必须从头文件抄录，绝不能凭记忆写**——写错不会报编译错误，只会静默返回空值或错误数据，是最难查的一类 bug。

**Files:**
- Create: `crates/probe/src/wpd/mod.rs`
- Create: `crates/probe/src/wpd/keys.rs`
- Modify: `crates/probe/src/main.rs`

- [ ] **Step 1: 先确认 `windows` crate 是否已导出这些常量**

Run:
```powershell
cargo doc -p probe --no-deps
```
然后打开 `target\doc\probe\...`，或在源码里试写：

```rust
#[cfg(windows)]
fn _check() {
    use windows::Win32::Devices::PortableDevices::WPD_OBJECT_NAME;
}
```

Run: `cargo build -p probe`
Expected: 两种结果之一——编译通过（crate 已导出，跳过 Step 2 直接用），或报 `cannot find value WPD_OBJECT_NAME`（进入 Step 2 手工声明）。

在源码里 grep 更快：
```powershell
Get-ChildItem "$env:USERPROFILE\.cargo\registry\src" -Recurse -Filter "*.rs" -ErrorAction SilentlyContinue |
  Select-String -Pattern "WPD_OBJECT_NAME" -List |
  Select-Object -First 5 -ExpandProperty Path
```

- [ ] **Step 2: 从 PortableDevice.h 抄录常量**

打开 Task 2 记下的 `PortableDevice.h` 路径，搜索下列宏（`DEFINE_PROPERTYKEY(...)` 形式），把 **GUID 与 PID 原样抄录**进 `crates/probe/src/wpd/keys.rs`：

需要的键（用途见括号）：

| 宏名 | 用途 |
|---|---|
| `WPD_OBJECT_ID` | 对象 ID |
| `WPD_OBJECT_PERSISTENT_UNIQUE_ID` | 稳定唯一标识（用于增量比对） |
| `WPD_OBJECT_NAME` | 对象显示名 |
| `WPD_OBJECT_ORIGINAL_FILE_NAME` | 原始文件名（**取扩展名用这个**） |
| `WPD_OBJECT_SIZE` | 文件字节数 |
| `WPD_OBJECT_DATE_CREATED` | 创建时间 |
| `WPD_OBJECT_DATE_MODIFIED` | 修改时间 |
| `WPD_OBJECT_FORMAT` | 格式码（判断图片/视频） |
| `WPD_OBJECT_CONTENT_TYPE` | 内容类型 |
| `WPD_OBJECT_CAN_DELETE` | 是否可删（只读校验用） |
| `WPD_DEVICE_FRIENDLY_NAME` | 设备显示名 |
| `WPD_DEVICE_MODEL` | 设备型号 |
| `WPD_DEVICE_SERIAL_NUMBER` | 设备序列号 |
| `WPD_STORAGE_CAPACITY` | 存储总容量 |
| `WPD_STORAGE_FREE_SPACE_IN_BYTES` | 剩余空间（iCloud 检测的候选信号） |

文件骨架（PID 值留空，**由 Step 2 从头文件填入**）：

```rust
//! WPD PROPERTYKEY 常量。
//!
//! 所有数值抄录自 Windows SDK 的 `PortableDevice.h`（`DEFINE_PROPERTYKEY` 宏）。
//! 来源路径记录于 docs/superpowers/plans/2026-09-17-stage0-wpd-probe.md 的 Task 2。
//! 修改前请重新核对头文件——这些值写错不会报错，只会静默返回错误数据。

use windows::core::GUID;
use windows::Win32::Foundation::PROPERTYKEY;

/// 抄录自 PortableDevice.h。用法：
/// `const WPD_OBJECT_NAME: PROPERTYKEY = propertykey(0xEF6B490D, 0x5CD8, 0x437A, [0xAF, 0xFC, 0xDA, 0x8B, 0x60, 0xEE, 0x4A, 0x3C], 4);`
const fn propertykey(
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
    pid: u32,
) -> PROPERTYKEY {
    PROPERTYKEY {
        fmtid: GUID {
            data1: d1,
            data2: d2,
            data3: d3,
            data4: d4,
        },
        pid,
    }
}

// TODO(Task 7 Step 2): 在此逐条声明上表中的 PROPERTYKEY。
// 每条格式：
// pub const WPD_OBJECT_ORIGINAL_FILE_NAME: PROPERTYKEY =
//     propertykey(0x____, 0x____, 0x____, [0x__, ...], __);
```

- [ ] **Step 3: 加一个自检测试**

在 `keys.rs` 末尾加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_keys_are_distinct() {
        // 若两个键的 GUID+PID 完全相同，说明抄录时复制粘贴出了错
        let all: Vec<(&str, PROPERTYKEY)> = vec![
            ("WPD_OBJECT_ID", WPD_OBJECT_ID),
            ("WPD_OBJECT_ORIGINAL_FILE_NAME", WPD_OBJECT_ORIGINAL_FILE_NAME),
            ("WPD_OBJECT_SIZE", WPD_OBJECT_SIZE),
            ("WPD_OBJECT_DATE_CREATED", WPD_OBJECT_DATE_CREATED),
            ("WPD_DEVICE_SERIAL_NUMBER", WPD_DEVICE_SERIAL_NUMBER),
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let (na, ka) = &all[i];
                let (nb, kb) = &all[j];
                assert_ne!(
                    (ka.fmtid, ka.pid),
                    (kb.fmtid, kb.pid),
                    "{na} 与 {nb} 的 PROPERTYKEY 相同，抄录有误"
                );
            }
        }
    }

    #[test]
    fn fmtids_are_non_zero() {
        assert_ne!(WPD_OBJECT_ID.fmtid.data1, 0, "GUID 未填充");
    }
}
```

> 这个测试能抓出"复制粘贴把某两条抄成一样"这类抄录错误，但抓不出"抄错了某个十六进制位"。后者只能靠交叉核对头文件。

- [ ] **Step 4: 建 `wpd/mod.rs` 并接进 main**

Create `crates/probe/src/wpd/mod.rs`:

```rust
#[cfg(windows)]
pub mod keys;
```

在 `main.rs` 顶部加：

```rust
#[cfg(windows)]
mod wpd;
```

- [ ] **Step 5: 运行测试**

Run: `cargo test -p probe`
Expected: PASS（Task 6 的 17 个 + 本任务 2 个 = 19 个）。

- [ ] **Step 6: 提交**

```bash
git add crates/probe/src/wpd crates/probe/src/main.rs
git commit -m "feat(probe): declare WPD property keys transcribed from PortableDevice.h"
```

---

## Task 8: `DeviceSource` 抽象与 WPD 设备枚举

**Files:**
- Create: `crates/probe/src/source.rs`
- Create: `crates/probe/src/wpd/device.rs`
- Modify: `crates/probe/src/wpd/mod.rs`
- Modify: `crates/probe/src/main.rs`

- [ ] **Step 1: 定义抽象边界**

Create `crates/probe/src/source.rs`:

```rust
use crate::model::{RemoteFile, StorageInfo};

/// 设备访问的抽象边界。
///
/// 纯逻辑（配对、报告）只依赖这个 trait，不依赖任何 Win32 类型，
/// 因此可以用假实现完整单元测试，也便于将来接入"本地备份解析"等替代通道。
pub trait DeviceSource {
    /// 设备友好名、型号、序列号
    fn device_info(&self) -> anyhow::Result<DeviceInfo>;

    /// 设备上的所有存储卷
    fn storages(&self) -> anyhow::Result<Vec<StorageInfo>>;

    /// 遍历相册目录，返回所有媒体文件
    fn list_media(&self) -> anyhow::Result<Vec<RemoteFile>>;
}

#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub friendly_name: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
}
```

把 `StorageInfo` 加到 `model.rs`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    pub object_id: String,
    pub name: String,
    pub capacity: Option<u64>,
    pub free_space: Option<u64>,
}
```

（并在 `main.rs` 加 `mod source;`）

- [ ] **Step 2: 写盘时序列号打码**

在 `model.rs` 加一个纯函数 + 测试：

```rust
/// 序列号打码：保留后 4 位，其余用 * 替代。诊断报告用。
pub fn mask_serial(serial: &str) -> String {
    let chars: Vec<char> = serial.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len());
    }
    let keep = chars.len() - 4;
    format!("{}{}", "*".repeat(keep), chars[keep..].iter().collect::<String>())
}

#[cfg(test)]
mod mask_tests {
    use super::mask_serial;

    #[test]
    fn masks_all_but_last_four() {
        assert_eq!(mask_serial("F17ABC3F9A2C"), "********9A2C");
    }

    #[test]
    fn masks_short_serial_entirely() {
        assert_eq!(mask_serial("AB"), "**");
        assert_eq!(mask_serial(""), "");
    }
}
```

Run: `cargo test -p probe mask`
Expected: PASS。

- [ ] **Step 3: 实现 WPD 设备枚举**

Create `crates/probe/src/wpd/device.rs`。核心流程（**实现时对照 MS Learn 的 Portable Devices 文档逐句核对，不要凭记忆**）：

1. `CoInitializeEx(None, COINIT_APARTMENTTHREADED)` 初始化 COM（`windows` crate 0.58 起 `CoInitializeEx` 签名可能已变，以编译报错为准调整）
2. `PortableDeviceManager::new()` → `EnumDevices(...)` 拿到设备 ID 列表
3. 对每个 ID：`GetDeviceFriendlyName`、`GetDeviceDescription`（型号）、`GetDeviceManufacturer`
4. `PortableDevice::new()` → `Open(device_id, None)` → `Content()` → 拿到 `IPortableDeviceContent`
5. 序列号：`Content.Properties()` → `GetValues` 查 `WPD_DEVICE_SERIAL_NUMBER`
6. 存储卷：`Content.EnumObjects(...)` 用 `WPD_DEVICE_OBJECT_ID` 作父，过滤功能类别为 `WPD_FUNCTIONAL_CATEGORY_STORAGE` 的对象

**必须遵守的约束：**
- **只读**。整个 `wpd/` 目录下**不得出现** `Delete`、`CreateObject`、`Move`、`Copy` 等写操作。这是探测程序，写操作有损坏用户相册的风险。
- 每个 COM 调用的 `HRESULT` 都要检查并带上上下文返回（`anyhow::Context`）。
- 序列号在进入 `DeviceInfo` 之前就打码——**不要把原始序列号传到任何会打印的地方**。

- [ ] **Step 4: 加一条"只读"守卫测试**

在 `main.rs` 或 `wpd/mod.rs` 加：

```rust
#[cfg(test)]
mod readonly_guard {
    #[test]
    fn wpd_module_contains_no_write_operations() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/wpd");
        let mut offenders = Vec::new();
        for entry in walkdir(dir) {
            let text = std::fs::read_to_string(&entry).unwrap();
            for needle in ["Delete", "CreateObject", "CopyHere", "Move("] {
                if text.contains(needle) {
                    offenders.push(format!("{entry}: {needle}"));
                }
            }
        }
        assert!(offenders.is_empty(), "探测程序必须是只读的，发现写操作: {offenders:?}");
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

> 注：`read_to_string` 对 UTF-8 以外的文件会失败——本仓库源文件统一 UTF-8，可接受。

Run: `cargo test -p probe readonly`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/probe/src/source.rs crates/probe/src/wpd crates/probe/src/model.rs crates/probe/src/main.rs
git commit -m "feat(probe): add DeviceSource trait, WPD device enumeration, readonly guard"
```

---

## Task 9: DCIM 遍历与元数据读取

**Files:**
- Modify: `crates/probe/src/wpd/device.rs`

- [ ] **Step 1: 实现递归枚举与属性读取**

在 `device.rs` 中实现 `DeviceSource::list_media`：

1. 从设备根对象 ID 出发，用 `IPortableDeviceContent::EnumObjects` + `IPortableDeviceEnumPortableDeviceObjectIDs::Next` 递归
2. 对每个对象，用 `IPortableDeviceProperties::GetValues` 批量取 Task 7 声明的属性键
3. 文件名优先取 `WPD_OBJECT_ORIGINAL_FILE_NAME`，为空则回退 `WPD_OBJECT_NAME`
4. 格式判定：`WPD_OBJECT_FORMAT` 落在图片格式码区间 → `FileKind::Image`；视频格式码区间 → `FileKind::Video`；其余 → `FileKind::Other`
5. **只递归进以下目录**（白名单，避免遍历整个设备）：`DCIM` 及其子目录。其余一律跳过，也不要读取非媒体对象的属性（省时间）

**参考：** 属性读取的数组与 `PROPVARIANT` 转换是这段最容易出错的地方。`windows` crate 提供了 `PROPVARIANT` 的辅助方法（如 `.to_string()`、`.as_u64()`），以编译期的实际方法名为准，不要凭记忆写。

- [ ] **Step 2: 加一个"元数据缺失时不 panic"的测试**

属性可能缺失（不同 iOS 版本返回的字段不同）。加测试验证缺字段时的行为——把属性读取的结果转成 `RemoteFile` 的那段逻辑抽成纯函数，便于测试：

```rust
/// 把可能缺失的属性组合成 RemoteFile。缺失字段用 None，绝不 panic。
pub fn build_remote_file(
    object_id: String,
    name: Option<String>,
    size: Option<u64>,
    date_created: Option<String>,
    kind: FileKind,
) -> RemoteFile {
    RemoteFile {
        object_id,
        name: name.unwrap_or_default(),
        size: size.unwrap_or(0),
        date_created,
        kind,
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;

    #[test]
    fn missing_name_and_size_do_not_panic() {
        let f = build_remote_file("o1".into(), None, None, None, FileKind::Other);
        assert_eq!(f.name, "");
        assert_eq!(f.size, 0);
        assert_eq!(f.base_name(), "");
    }
}
```

Run: `cargo test -p probe`
Expected: PASS。

- [ ] **Step 3: 提交**

```bash
git add crates/probe/src/wpd/device.rs
git commit -m "feat(probe): enumerate DCIM and read object metadata"
```

---

## Task 10: 串联 CLI 并实测

**Files:**
- Modify: `crates/probe/src/main.rs`

- [ ] **Step 1: 实现 main**

`main.rs` 的流程：

1. 解析参数：`--json`（输出 JSON 而非人类可读文本）、`--out <路径>`（写入文件而非 stdout）
2. `CoInitializeEx`（入口处调用一次，注意 Task 8 Step 3 的签名差异）
3. 构造 WPD `DeviceSource`；无设备时给出明确提示（"请插上手机、解锁、并点'信任此电脑'"）
4. `device_info()` → `storages()` → `list_media()` → `pairing::pair()` → `report::build_report()`
5. 输出 `render_human()` 或 `serde_json::to_string_pretty()`
6. 退出码：`go == true` 返回 0，否则返回 2（便于脚本判定）

- [ ] **Step 2: 编译**

Run: `cargo build -p probe --release`
Expected: 编译成功。

- [ ] **Step 3: 无设备时的行为**

拔掉手机，Run: `cargo run -p probe --release`
Expected: 打印"未发现设备，请插上手机…"，退出码非 0，**不 panic、不打印堆栈**。

- [ ] **Step 4: 插上手机实测**

手机插上、解锁、点"信任此电脑"，Run:
```powershell
cargo run -p probe --release -- --out "$env:TEMP\probe-report.txt"
Get-Content "$env:TEMP\probe-report.txt"
```

Expected: 报告中 `成对(实况)` > 0，结论为"通道可行"。

- [ ] **Step 5: 与 Task 0 的 PowerShell 结果交叉核对**

把本任务的「文件总数」与 Task 0 脚本的「DCIM 下文件总数」比对。

- **若两者一致** → WPD 层实现正确，可信。
- **若 WPD 报的明显更少** → WPD 层漏了对象（可能是递归白名单过窄，或 `Next` 分页处理有 bug）。这是必须查的 bug，不能带着过。
- **若 WPD 报的明显更多**（比如多出 `.AAE`、`.XMP` 等）→ 正常，PowerShell 脚本可能过滤了子目录。

- [ ] **Step 6: 把结论写进设计文档 §2.3**

```bash
git add docs/superpowers/specs/2026-09-17-liveporter-design.md
git commit -m "docs: record stage-0 WPD probe result and cross-check"
```

- [ ] **Step 7: 提交 CLI**

```bash
git add crates/probe/src/main.rs
git commit -m "feat(probe): wire up CLI with human and json output"
```

---

## 完成判据

本计划在以下全部成立时才算完成：

- [ ] Task 0 与 Task 10 两次独立探测都确认 `.MOV` 被暴露
- [ ] WPD 探测的文件数与 PowerShell 探测交叉核对一致
- [ ] `cargo test -p probe` 全绿（约 23 个测试：model 5 + pairing 6 + report 6 + keys 2 + mask 2 + readonly 1 + build 1）
- [ ] 探测程序在无设备时优雅退出，不 panic
- [ ] 设计文档 §2.3 的未知项已更新为实测结论
- [ ] 记录下实测得到的传输速率与并发流数量的初步观察（为设计文档 §12 的对应风险项提供数据）

## 明确不在本计划内

- 不读取任何文件的实际字节内容（不下载照片）
- 不写数据库、不建库目录
- 不做转码、不碰 ffmpeg
- 不做任何 Tauri / 前端工作
- 不安装 WebView2 相关工具链

这些都留给计划 2 及之后。

---

## 自查记录

**规格覆盖：** 对应设计文档 §12 的致命风险项与 §13 的实施顺序第 1 步，以及附录 B 待实测清单的第 1、3 项。第 2、4、5 项（iCloud 检测手段、ContentIdentifier 读取、ffmpeg 裁剪）不在本计划范围内，分别属于后续计划与计划 3。

**类型一致性：** `FileKind` / `RemoteFile` / `Pair` / `PairStatus` / `ProbeReport` / `DeviceInfo` / `StorageInfo` 在 Task 4~9 中签名一致；`build_remote_file` 的返回值与 `RemoteFile` 字段对齐。

**已知的不确定点（实现时需查证，不得凭记忆写）：**
1. `windows` crate 的 WPD feature 名称与 `CoInitializeEx` 的确切签名（Task 3 / Task 8）
2. `WPD_*` PROPERTYKEY 的具体 GUID 与 PID —— **必须抄头文件**（Task 7）
3. `IPortableDeviceEnumPortableDeviceObjects::Next` 的参数形状（Task 9）
4. `PROPVARIANT` 在 `windows` crate 中的取值辅助方法名（Task 9）

以上四点都在计划里标了明确的查证动作，不要跳过。

**对 writing-plans 规范的一处有意偏离：**

规范要求"每个步骤都要给出完整代码，不许出现 TBD"。本计划在 **Task 7 的 PROPERTYKEY 声明**和 **Task 8 Step 3 的 WPD COM 实现**两处，只给了骨架和查证动作，没给可直接复制的完整代码。

这是刻意的，理由是：**这两处的正确内容只能来自权威来源，不能来自记忆。** PROPERTYKEY 是 128 位 GUID + 32 位 PID 的字面量，写错一位不会报编译错误，只会静默返回空值——是最难排查的一类 bug。WPD COM 的调用序列和参数形状同理。

如果在这里凭印象写出"看起来对"的代码，产出的是一份**看起来完整、实际会跑错**的计划，比明确标出"此处需查证"更危险。因此这两处的"完整内容"被替换为**明确的查证动作 + 权威来源路径**（Task 2 抄录的头文件路径、MS Learn 的 Portable Devices 文档）。

执行者在动手前必须先完成查证。其余所有任务（Task 0~6、9 的纯逻辑部分）都给出了可直接运行的完整代码。