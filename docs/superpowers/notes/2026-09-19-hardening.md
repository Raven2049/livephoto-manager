# 整体审查与发布后加固（2026-09-19）

对 v1.0.0 做了一轮整体审查，范围：**UI/UX 对照 HIG**、**打包分发与安全**、**功能完整度与设计一致性**。
本文记录审查结论与随后落地的修改。审查时的完整问题清单见对话记录；此处只留已改动项与待办。

## 已落地

### 1. 可访问性（P0）

依据 `apple-hig`：`accessibility.md:51`（Increase Contrast）、`:89`（Full Keyboard Access）、`:96`（避免定时自动消失）。

- **导入开关**改原生 `<input type="checkbox" role="switch" v-model>`（视觉隐藏），去掉 `@click.prevent`；
  焦点环画在 `.track` 上。现在可 Tab 进入、空格切换、屏幕阅读器可读出开关状态与标签。
  - 位置：`src/App.vue`（模板「从 iPhone 导入」区）、`src/styles/app.css`（`.switch-input`、`:focus-visible + .track`）。
- **对比度**：`.group-title` / `.sec-title` / `.menu-label` / `.row-chevron` / `.sidefoot` / `.pop-title`
  由 `--label-3` 提升到 `--label-2`；并新增 `@media (prefers-color-scheme) and (prefers-contrast: more)`
  两段覆盖，在系统开启对比度增强时进一步加深 `--label-2/--label-3/--separator`（WebView2 会把 Windows
  对比度设置映射到 `prefers-contrast`）。
  - 位置：`src/styles/app.css`、`src/App.vue`、`src/components/FilterBar.vue`。
- **错误 toast 不再自动消失**，改为常驻、点击关闭（`role=alert`）；信息 toast 仍 4s 收起。
  - 位置：`src/App.vue`。

### 2. 安全收紧

依据：`protocol.rs` 的白名单根取自当前库，过宽会放大注入后的可读范围。

- **`lpm://` 白名单收窄**：只服务 `originals/ thumbs/ previews/ larges/ .lpm/view` 五个子目录，且扩展名必须
  通过 `pairing::is_still_name || is_movie_name`（复用现有清单，避免二份维护）。显式挡住库根的裸媒体文件、
  `.lpm/index.db`、`.lpm/diagnostics-*.txt`。路径仍走 `canonicalize` + 子目录前缀双重判定（`..`/符号链接）。
  - 位置：`src-tauri/src/protocol.rs`。新增 5 个单测。
- **`open_library` 校验**：新增 `validate_library_root`，拒绝盘根（无父目录）与
  `SystemRoot` / `ProgramFiles` / `ProgramFiles(x86)` / `ProgramData` 及其上级；仍允许任意普通目录。
  - 位置：`src-tauri/src/commands.rs`。新增 4 个单测。

### 3. integrity 持久化

问题：`indexer` 重扫按「文件是否在场」配对并 upsert，`ON CONFLICT` 直接覆盖 `integrity`，会把导入/校验得到的
`1/2/5` 抹成 `0/3/4`；且 `classify_library` 硬编码 `cloud_hint=false`，`integrity=5` 永远不可达。

- 重扫时先算「在场形态」（3 仅静态 / 4 仅视频 / 其余视为两侧都在）：**形态不变则保留旧分类**，
  形态变化（如视频被删）才用新形态值覆盖。
  - 位置：`src-tauri/src/db.rs::upsert_asset_preserving_taken_at` + `presence_shape`。
- `classify_library` 新增 `assume_cloud`，前端复用侧栏「原件可能不在手机」开关。
  - 位置：`src-tauri/src/commands.rs`、`src/stores/library.ts`、`src/App.vue`。
- 新增回归测试：`rescan_preserves_validated_integrity`、`rescan_updates_integrity_when_shape_changes`。

### 4. 打包治理

- **ffmpeg 版本/哈希锁定**：新增 `scripts/ffmpeg.sha256`（记录裁剪版 `ffmpeg.exe`/`ffprobe.exe` 的
  SHA-256，当前对应 FFmpeg 9.0.1 裁剪构建）；`scripts/package-portable.ps1` 在打包前强校验，不一致即失败，
  并提供 `-RecordFfmpegHash` 重建清单、`-SkipFfmpegHashCheck` 跳过。
- **依赖门禁**：`scripts/deps-check.ps1` 新增 `-FailOnFound`（发现非系统 DLL 以 2 退出）；
  `package-portable.ps1` 调用它并在非 0 时中断打包（`-SkipDepsCheck` 可跳过）。
- **许可文本**：`THIRD_PARTY_NOTICES.md` 补入 libwebp 的完整 BSD-3-Clause 文本，以及 libx264
  （GPL-2.0-or-later，经「or later」纳入 GPL-3.0-or-later）的声明与规范文本 URL。随包分发的
  `LICENSE`（GPLv3 全文）+ 本声明即覆盖。
- **干跑验证**：`package-portable.ps1 -SkipBuild` 实跑通过（哈希 OK → 依赖 OK → 组装 → zip + SHA-256）。

## 验证

- `cargo test`：**81 passed / 0 failed / 9 ignored**（liveporter）+ **25 passed**（probe）。
- `cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 通过。
- `npm run build`（`vue-tsc --noEmit && vite build`）通过。
- **未做实机 GUI 复验**：开关的焦点环、对比度、toast 关闭行为需在 `tauri dev` 目视确认一次。

## 发布后待办（本次未做）

1. **iPhone「保留原件」自动检测**：信号已确认（设备端静态图全 `.JPG`、无 `.HEIC`），**尚未实现**；
   iCloud「优化 iPhone 储存空间」检测手段仍未验证。实现前不要臆断。见设计文档 §6.4 / 附录 B。
2. **CSP**：`tauri.conf.json` 的 `security.csp` 仍为 `null`。需同时放行 `lpm.localhost` / IPC / HMR，
   且必须 `tauri dev` 实机确认图片与视频仍能加载，故暂缓。
3. ~~**打包治理**：ffmpeg 未锁定版本/哈希；`deps-check.ps1` 未接入打包且非系统 DLL 仍以 0 退出；
   `THIRD_PARTY_NOTICES.md` 缺 libx264（GPLv2）与 libwebp（BSD）的完整文本。~~ **已完成（见上「4. 打包治理」）**。
   仅余 ffmpeg 二进制**位级可复现**未解决。
4. **文档一致性**：本轮已修正「19 项决策」「计划 9 尚待实现」、测试数、`larges/`/`.lpm/view` 缺失等；
   `docs/releasing.md` 引用的 `release-notes-1.0.0.md` 已补建。
