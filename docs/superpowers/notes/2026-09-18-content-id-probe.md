# 阶段 6 Task 0：ContentIdentifier 读取路径真机验证（2026-09-18）

用无界面测试下载了一对真实实况素材（`IMG_1717.MOV` 350,011 B + `IMG_1717.HEIC` 1,264,462 B），
分别验证视频与静态图两条读取路径。

## 视频侧：ffprobe 直接可用

```powershell
ffprobe -v error -show_entries format_tags=com.apple.quicktime.content.identifier -of default=nw=1:nk=1 IMG_1717.MOV
# => 4700E756-B41E-4303-A840-DDD273FE90D4
```

ffprobe 把它解析成 **format tag**。同一 MOV 还可见：
`com.apple.quicktime.live-photo.auto=1`、`...vitality-score` 等 Apple Live Photo 元数据。

## 静态图侧：签名扫描 + MakerNote IFD 解析可用

ffprobe **读不到** HEIC 的 MakerNote（只会给 `major_brand`/tile 信息）。
改用「在文件字节里扫描签名 `"Apple iOS\0"` 再按 TIFF 风格 IFD 解析」：

- 命中 **1 处**，位于字节偏移 `5145`
- 字节序 `MM`（大端）
- IFD 条目数 50
- tag `0x0011`（type=ASCII）的值 = `4700E756-B41E-4303-A840-DDD273FE90D4`

## 结论

**两条路径都成立，且 UUID 完全一致。** 计划中「签名扫描」的启发式做法在真实 iPhone HEIC 上有效，
无需完整解析 HEIF 容器（`iinf`/`iloc`）。可以按计划实现 `livephoto.rs`。
