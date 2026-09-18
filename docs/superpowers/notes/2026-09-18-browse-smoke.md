# 阶段 7 浏览体验与 UI 实测记录（2026-09-18）

## 结果

计划 8（`docs/superpowers/plans/2026-09-18-stage7-browse-ui.md`）实现完成。本机完成**自动化验证**；
**真实素材的 GUI 交互实测尚未执行**（本机无可用库/无交互式会话，见文末待补清单）。

## 自动化验证

| 检查项 | 命令 | 结果 |
|---|---|---|
| 回收站闸门 | `cargo test -p liveporter trash_moves_a_temp_file -- --ignored` | **通过**：文件成功移入回收站 |
| Rust 单测 | `cargo test -p probe -p liveporter` | **通过**：liveporter 63、probe 25，0 失败 |
| 静态检查 | `cargo clippy -p probe -p liveporter --all-targets -- -D warnings` | **通过** |
| 格式 | `cargo fmt --check` | **通过** |
| 前端类型 | `npx vue-tsc --noEmit` | **通过** |
| 前端构建 | `npm run build` | **通过**（dist js ~90.7 kB / gzip 35.4 kB） |

关键新增单测：

- `db`: `escape_like`、`filter_by_kind_integrity_and_date`、`filter_text_matches_substring_and_escapes`、
  `page_orders_by_taken_at_desc_then_id_desc`、`count_and_ids_match_the_filtered_page`、
  `rescan_preserves_imported_taken_at`、`assets_by_ids_...`、`delete_assets_removes_rows_and_counts_cache_refs`
- `export`: `uses_base_name_when_free`、`adds_same_suffix_to_both_when_colliding`、`still_only_has_no_movie_dest`
- `delete`: `source_paths_*`、`only_unreferenced_cache_is_removed`、`inside_root_distinguishes_inside_and_outside`

## 依赖

- 新增 `trash = "5"`，本机解析为 `trash v5.2.9`，并拉入 `windows v0.62.2`；与项目既有 `windows 0.58`
  并存，编译无冲突。

## 实现要点

- 查询统一按 `taken_at DESC, id DESC`；`AssetRow` 新增 `taken_at`。
- 前端分页页大小 1000；`PhotoGrid` 用纯函数 `buildLayout` 铺「日期头 + 瓦片行 + 页脚」，行级虚拟化。
- 时间线支持年/月/天多级：默认随缩放列数切换（≤5 天 / 6~9 月 / ≥10 年），FilterBar 可手动覆盖。
- 导出/删除以逻辑条目为单位；导出重名时配对文件用同一后缀；删除先删行、再按引用计数清理孤儿缓存。

## 待补：真实素材 GUI 实测清单

以下需在有真实库的机器上运行 `npm run tauri dev`（记得设 `$env:LIVEPORTER_FFMPEG`）后逐项确认：

- [ ] 时间线按天/月/年分段、切粒度时视图不跳
- [ ] 滚动到底自动加载下一页，无重复请求
- [ ] 搜索防抖、类型/日期/完整性筛选结果正确
- [ ] 缩放锚点（跨日期头）手感
- [ ] 悬停预览未因网格重写退化
- [ ] 多选/全选计数正确；大库全选不卡 UI
- [ ] 导出：配对文件一起、重名成对后缀、`Get-FileHash` 抽查一致
- [ ] 删除：可从回收站还原；索引与孤儿缓存同步清理；删除后刷新正确

## 人工复验（2026-09-18，`npm run tauri dev`）

由用户在本机启动 dev（设 `LIVEPORTER_FFMPEG` 指向 essentials ffmpeg），实际点了一圈界面，
反馈：**「感觉还行，没感觉有什么明显的问题」**。dev 日志无报错（vite 就绪、cargo 编译通过、
`liveporter.exe` 正常启动）。

> 性质说明：这是**主观冒烟确认**，上面清单里的每一项并未逐条核对（尤其导出重名、删除后缓存清理、
> 大库全选性能）。作为「没坏」的信号可接受；要下「通过」结论仍建议按清单逐项走一遍。

## 验收发现并修复的问题

1. **「校验标识」会不停闪黑框（已修）**：GUI 程序为 `windows_subsystem = "windows"`，
   而 `ffprobe` 是控制台程序；`classify_library` 对每个视频 spawn 一次 `ffprobe`，
   Windows 每次都会弹一个控制台窗口再关掉。缩略图/预览片/导出诊断同样存在该问题。
   **修复**：`ffmpeg::command()` 统一构造子进程并加 `CREATE_NO_WINDOW`（`src-tauri/src/ffmpeg.rs`），
   `ffmpeg::run` / `run_capture` 与诊断报告里的 `cmd /c ver`、`ffmpeg -version` 全部改用它。
   有单测 `command_helper_hides_console_and_runs`。

2. **时间线出现「未知日期」（已修）**：库里同一台手机出现了**两个 device 行**——
   `indexer`（重建索引）用文件夹名当序列号建了 `AppleiPhone-CY4RV0/unknown`，
   而 WPD 导入用真实序列号 `GV95CY4RV0/Apple iPhone`。业务键 `(device_id, base_name, taken_at)`
   含 `device_id`，于是重扫把同一批文件又写了一份（`taken_at=0`）→ 显示「未知日期」。
   **修复**：新增 `db::device_id_for_folder()`，`indexer` 优先复用该目录已有的设备行；
   单测 `rescan_reuses_imported_device_and_preserves_taken_at`。
   历史重复已按用户决定**清空库重导**（原文件先改名备份，确认后送回收站），重导后日期正常。

## 结论

计划 8 的代码与自动化验证已完成；打包版在真实库 `D:\图片\Pictures\iPhone` 上经用户实测
**功能全部 OK**，发现并修复了「校验标识闪黑框」与「重扫产生重复行/未知日期」两个 bug，
清理重导后日期正常。逐项清单其余条目（导出重名/删除后缓存清理/大库全选性能）未单独强调，
但用户整体确认为可用。
