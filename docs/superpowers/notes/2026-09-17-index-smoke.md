# 阶段 2 索引冒烟实测（2026-09-17）

## 目的

验证计划 3（SQLite 索引 + 库目录管理）的端到端链路：打开库 → 建目录结构 → 扫描 `originals/` → 配对写索引 → UI 从索引读数据。

## 测试库构成

手工造在 `%TEMP%\opencode\testlib`（模拟将来设备导入后的样子）：

```
testlib/
└── originals/
    └── iPhone15Pro-3F9A2C/
        └── 2024/
            ├── IMG_0001.JPG … IMG_0020.JPG   （20 张真实 JPG）
            ├── IMG_0001.MOV … IMG_0010.MOV   （10 个占位 MOV，与同名 JPG 配对）
            ├── IMG_0021.MOV                   （孤立视频）
            └── notes.txt                      （非媒体，应被忽略）
```

预期：`设备 1 · 文件 32 · 条目 21`，其中 `实况 10 · 照片 10 · 视频 1`。

## 结果（用户实测确认）

| 检查项 | 结果 |
|---|---|
| 打开库后目录结构建立（`originals/ thumbs/ previews/ .lpm/`） | 正常 |
| 「重建索引」后统计栏数字 | **符合预期**（共 21 · 实况 10 · 照片 10 · 视频 1 · 缺失 0） |
| 网格显示索引中的 JPG | 正常（20 张） |
| 再次「重建索引」数字不变（幂等） | 符合预期 |

用户结论：**符合预期**。

> `notes.txt` 被忽略、`IMG_0021.MOV` 记为视频、`IMG_0001..0010` 记为实况——说明非媒体过滤与配对都正确。

## 由单元测试覆盖、UI 未单独手测的部分

- **删文件后重建 → `missing` 计数增加**：由 `indexer::tests::missing_is_set_when_file_disappears` 覆盖（造临时目录、删文件、重扫、断言 `missing=1`）。
- **重扫幂等 / 不产生重复行**：由 `indexer::tests::scans_and_indexes_idempotently` 覆盖。
- **越界路径拒绝**：由 `protocol::tests::resolve_rejects_path_outside_root` 覆盖。

## 已知限制（计划 3 文档已记录，此处复述）

1. `integrity=0` 是**未经 UUID 校验**的临时值；`content_id` 为 NULL。计划 5 补齐校验后可能降级为 1/2。
2. `taken_at` 暂为 0，业务键实际退化为 `(device_id, base_name)`。
3. `device.serial` 用设备目录名占位（本计划无设备元数据）。
4. 索引里存**绝对路径**；库被移动后这些路径会失效。
