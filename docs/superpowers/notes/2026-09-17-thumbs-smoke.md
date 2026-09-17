# 阶段 4 缩略图实测（2026-09-17）

## 环境

- ffmpeg **9.0.1-essentials_build**（gyan.dev），含 `libwebp` 编码器与 `hevc` 解码器。
- 库：`D:\图片\Pictures\iPhone`（设备：Apple iPhone）。

## 结果

| 检查项 | 结果 |
|---|---|
| HEIC 解码 → 512px WebP | 成功（真实 iPhone HEIC：38 KB webp） |
| MOV 取首帧 → WebP | 成功（6 KB webp） |
| 导入时生成缩略图 | 成功；条目推进到 `status=2 (transcoded)` |
| `thumb_path` 写入 DB | 成功 |
| 缩略图文件有效 | 是（文件头 `RIFF....WEBP`） |
| `thumbs/` 内容哈希去重 | 生效（内容相同的条目共用一个 thumb 文件） |
| 网格显示缩略图（HEIC 不再破图） | **用户确认可见** |
| 拍摄时间解析（阶段 3 遗留） | **已修复**：文件落到 `originals/AppleiPhone-CY4RV0/2025/`，不再是 `unknown` |

## 过程中修的两个真实问题

1. **`PropVariantToDouble` 对 `VT_DATE` 失败**（阶段 3 遗留的 `unknown` 根因）。
   `WPD_OBJECT_DATE_CREATED` 的 `vt=0x0007`（VT_DATE），但双击转换返回失败；
   `PropVariantToBSTR` 能给出 `"2026/04/30:19:42:06.000"`。
   修复：`taken_at_from_pv` 改为「先试 double，失败则解析字符串」，并手写民用历→epoch。

2. **ffmpeg 缩略图必须用 `-filter_complex` 而非 `-vf`。**
   iPhone 的 HEIC 会暴露多个图像流，ffmpeg 内部构建复杂滤镜图，此时再用 `-vf` 报
   `Filtergraph 'scale=512:-2' was specified for a stream fed from a complex filtergraph`。
   实测：`-vf` 失败、`-filter_complex "scale=512:-2"` 成功。已改 `ffmpeg.rs` 并加注释。
   （注：**普通** HEIC 样例用 `-vf` 也能成功，所以这个坑只在 iPhone 的 HEIC 上暴露。）

## 已知测试残留（本次开发迭代造成，非产品缺陷）

1. **重复行**：同一批资源各有两行（`taken_at=0` 与真实值）。修复拍摄时间**之前**的那次导入
   用了 `taken_at=0`，业务键 `(device, base_name, taken_at)` 因此不同 → 两行。清库重导即可。
2. **`iPhone\originals\` 被当成另一个库打开过**：出现 `originals\.lpm`、`originals\originals\...`。
   是「打开库」时误选了 `originals` 子目录。
3. 一个锁定的残留文件 `originals\...\unknown\IMG_0032.HEIC`（删除时被占用）。

> 结论：这三项都建议**清空该库重导**消除；缩略图与年份目录功能本身已验证正确。

## 未做（按计划范围）

- 预览片与悬停播放（计划 6）
- UUID 校验、异常分类、诊断报告、iCloud 检测（计划 7）
- ffmpeg 裁剪构建与随包分发（计划 9）
