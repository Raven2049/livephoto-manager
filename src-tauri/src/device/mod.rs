//! 只读 WPD 设备访问。
//!
//! **线程约定：** WPD 的 COM 接口都是 `!Send` 的。所有对设备的调用必须在
//! **同一个已 `CoInitializeEx(COINIT_APARTMENTTHREADED)` 的专用线程**上执行
//! （见 `ComGuard`）。不要把这些接口搬到别的线程，也不要在 Tauri 主线程上直接调用
//! （`GetStream`/`Read` 会长时间阻塞）。
//!
//! **只读约束：** 本目录下只允许读操作。`lib.rs` 的 `readonly_guard` 测试在看守。

pub mod keys;
pub mod transfer;

use anyhow::{bail, Context, Result};
use windows::core::{BSTR, GUID, PCWSTR, PROPVARIANT, PWSTR};
use windows::Win32::Devices::PortableDevices::{
    IEnumPortableDeviceObjectIDs, IPortableDevice, IPortableDeviceContent,
    IPortableDeviceKeyCollection, IPortableDeviceManager, IPortableDevicePropVariantCollection,
    IPortableDeviceProperties, IPortableDeviceValues, PortableDevice, PortableDeviceFTM,
    PortableDeviceKeyCollection, PortableDeviceManager, PortableDevicePropVariantCollection,
    PortableDeviceValues, WPD_CONTENT_TYPE_FOLDER, WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT,
    WPD_CONTENT_TYPE_IMAGE, WPD_CONTENT_TYPE_VIDEO, WPD_DEVICE_OBJECT_ID,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

use keys::*;

const FETCH_CHUNK: usize = 128;

/// RAII：在调用线程上初始化/反初始化 COM（STA）。
pub struct ComGuard {
    active: bool,
}

impl ComGuard {
    pub fn new() -> Result<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // S_FALSE 表示本线程已初始化过，同样可用。
        if hr.is_err() {
            bail!("CoInitializeEx 失败: {hr:?}");
        }
        Ok(ComGuard { active: true })
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.active {
            unsafe { CoUninitialize() };
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Image,
    Video,
    Other,
}

#[derive(Debug, Clone)]
pub struct MediaFile {
    /// 稳定标识。iOS 的对象句柄会失效，传输前必须用它重新解析出新鲜句柄。
    pub persistent_id: String,
    pub name: String,
    pub size: u64,
    pub taken_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub friendly_name: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    /// 库里的设备目录名：`<model>-<serial 后6位>`。
    pub folder_name: String,
}

/// OLE Automation 日期 → Unix epoch 秒。取不到返回 None。
///
/// 实测（阶段 4）：iPhone 的 `WPD_OBJECT_DATE_CREATED` 是 `VT_DATE`（vt=0x0007），
/// 但 `PropVariantToDouble` 对它**返回失败**；而 `PropVariantToBSTR` 能得到
/// 形如 `"2026/04/30:19:42:06.000"` 的本地时间字符串。因此优先试 double，
/// 失败则解析字符串。
pub fn taken_at_from_pv(pv: &PROPVARIANT) -> Option<i64> {
    if let Ok(ole) = f64::try_from(pv) {
        if ole > 0.0 {
            return Some(((ole - 25569.0) * 86400.0) as i64);
        }
    }
    let s = BSTR::try_from(pv).ok()?.to_string();
    parse_date_string(&s)
}

/// 解析形如 `YYYY/MM/DD:HH:MM:SS(.mmm)` 的本地时间字符串为 epoch 秒。
/// 按“连续数字分组”取前 6 个整数，兼容分隔符差异。
pub fn parse_date_string(s: &str) -> Option<i64> {
    let mut nums: Vec<i64> = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            nums.push(cur.parse().ok()?);
            cur.clear();
        }
    }
    if !cur.is_empty() {
        nums.push(cur.parse().ok()?);
    }
    if nums.len() < 6 {
        return None;
    }
    let (y, m, d, hh, mm, ss) = (
        nums[0] as i32,
        nums[1] as i32,
        nums[2] as i32,
        nums[3],
        nums[4],
        nums[5],
    );
    Some(days_from_civil(y, m, d) * 86400 + hh * 3600 + mm * 60 + ss)
}

/// 民用历 (y,m,d) → 自 1970-01-01 起的天数（Howard Hinnant 算法）。
fn days_from_civil(y: i32, m: i32, d: i32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + (d as i64 - 1);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// 由 epoch 秒取 UTC 年份（差一天的边界对本用途可接受，不引 chrono）。
pub fn year_of(epoch: i64) -> i32 {
    let mut days = epoch.div_euclid(86400);
    let mut year = 1970i64;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if days < len {
            break;
        }
        days -= len;
        year += 1;
    }
    year as i32
}

pub struct WpdDevice {
    content: IPortableDeviceContent,
    properties: IPortableDeviceProperties,
    info: DeviceInfo,
}

impl WpdDevice {
    pub fn content(&self) -> &IPortableDeviceContent {
        &self.content
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// 打开第一台友好名含 Apple/iPhone 的设备。找不到返回 Err（含提示文案）。
    pub fn open_first() -> Result<Self> {
        unsafe {
            let manager: IPortableDeviceManager =
                CoCreateInstance(&PortableDeviceManager, None, CLSCTX_ALL)
                    .context("创建 PortableDeviceManager 失败")?;

            let mut count = 0u32;
            manager
                .GetDevices(std::ptr::null_mut(), &mut count)
                .context("枚举设备数量失败")?;
            if count == 0 {
                bail!("未发现设备：请插上手机、解锁、并在手机上点\"信任此电脑\"");
            }

            let mut ids = vec![PWSTR::null(); count as usize];
            manager
                .GetDevices(ids.as_mut_ptr(), &mut count)
                .context("枚举设备失败")?;

            let mut candidates: Vec<Vec<u16>> = Vec::new();
            for p in &ids {
                if p.is_null() {
                    continue;
                }
                let mut v = p.as_wide().to_vec();
                v.push(0);
                candidates.push(v);
            }
            for p in &ids {
                if !p.is_null() {
                    CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
                }
            }

            // 系统上可能混有其它 WPD 设备（实测有把本机磁盘映射成 WPD 的设备），
            // 对它们调用 Open 会无限期挂起。只打开友好名明确的 Apple/iPhone 设备。
            let mut apple: Vec<Vec<u16>> = Vec::new();
            let mut other = 0usize;
            for c in candidates {
                let name =
                    read_two_call(|p, l| manager.GetDeviceFriendlyName(PCWSTR(c.as_ptr()), p, l));
                match name {
                    Some(n)
                        if {
                            let l = n.to_ascii_lowercase();
                            l.contains("iphone") || l.contains("apple")
                        } =>
                    {
                        apple.push(c)
                    }
                    _ => other += 1,
                }
            }
            if apple.is_empty() {
                if other > 0 {
                    bail!("未发现 iPhone 设备（检测到 {other} 个非 Apple 的 WPD 设备，已跳过）：请插上手机、解锁、并在手机上点\"信任此电脑\"");
                }
                bail!("未发现设备：请插上手机、解锁、并在手机上点\"信任此电脑\"");
            }

            let client_info: IPortableDeviceValues =
                CoCreateInstance(&PortableDeviceValues, None, CLSCTX_ALL)
                    .context("创建 client info 失败")?;

            let mut opened: Option<(IPortableDevice, Vec<u16>)> = None;
            let mut last_err: Option<windows::core::Error> = None;
            'outer: for clsid in [PortableDevice, PortableDeviceFTM] {
                let device: IPortableDevice = match CoCreateInstance(&clsid, None, CLSCTX_ALL) {
                    Ok(d) => d,
                    Err(e) => {
                        last_err = Some(e);
                        continue;
                    }
                };
                for cand in &apple {
                    match device.Open(PCWSTR(cand.as_ptr()), Some(&client_info)) {
                        Ok(()) => {
                            opened = Some((device, cand.clone()));
                            break 'outer;
                        }
                        Err(e) => last_err = Some(e),
                    }
                }
            }
            let (device, pnp_id) = opened.ok_or_else(|| {
                anyhow::anyhow!(
                    "打开设备失败（尝试了 {} 个设备）: {}",
                    apple.len(),
                    last_err
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "无设备".into())
                )
            })?;

            let content = device.Content().context("获取设备内容失败")?;
            let properties = content.Properties().context("获取属性接口失败")?;

            let id = PCWSTR(pnp_id.as_ptr());
            let friendly_name = read_two_call(|p, l| manager.GetDeviceFriendlyName(id, p, l));
            let description = read_two_call(|p, l| manager.GetDeviceDescription(id, p, l));

            let dev_keys = key_collection(&[WPD_DEVICE_MODEL, WPD_DEVICE_SERIAL_NUMBER])?;
            let dev_values = content
                .Properties()?
                .GetValues(WPD_DEVICE_OBJECT_ID, &dev_keys)
                .ok();
            let model_prop = dev_values
                .as_ref()
                .and_then(|v| v.GetStringValue(&WPD_DEVICE_MODEL).ok())
                .and_then(|s| pwstr_to_string_and_free(s));
            let serial = dev_values
                .as_ref()
                .and_then(|v| v.GetStringValue(&WPD_DEVICE_SERIAL_NUMBER).ok())
                .and_then(|s| pwstr_to_string_and_free(s));

            let raw_model = model_prop
                .clone()
                .or(description.clone())
                .unwrap_or_else(|| "Device".into());
            let folder_name = folder_name_for(&raw_model, serial.as_deref());

            Ok(WpdDevice {
                content,
                properties,
                info: DeviceInfo {
                    friendly_name,
                    model: model_prop.or(description),
                    serial,
                    folder_name,
                },
            })
        }
    }

    /// 递归枚举整卷媒体（沿用阶段 0 的结论：没有 `DCIM`，要递归整卷）。
    pub fn list_media(&self) -> Result<Vec<MediaFile>> {
        unsafe {
            let keys = key_collection(&[
                WPD_OBJECT_NAME,
                WPD_OBJECT_ORIGINAL_FILE_NAME,
                WPD_OBJECT_SIZE,
                WPD_OBJECT_DATE_CREATED,
                WPD_OBJECT_CONTENT_TYPE,
                WPD_OBJECT_PERSISTENT_UNIQUE_ID,
            ])?;
            let mut out = Vec::new();
            self.walk(WPD_DEVICE_OBJECT_ID, &keys, &mut out)?;
            Ok(out)
        }
    }

    unsafe fn walk(
        &self,
        parent: PCWSTR,
        keys: &IPortableDeviceKeyCollection,
        out: &mut Vec<MediaFile>,
    ) -> Result<()> {
        let enumer: IEnumPortableDeviceObjectIDs = self
            .content
            .EnumObjects(0, parent, None::<&IPortableDeviceValues>)
            .context("EnumObjects 失败")?;

        loop {
            let mut buf = vec![PWSTR::null(); FETCH_CHUNK];
            let mut fetched = 0u32;
            let _ = enumer.Next(&mut buf, &mut fetched);
            if fetched == 0 {
                for p in &buf {
                    if !p.is_null() {
                        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
                    }
                }
                break;
            }

            for p in buf.iter().take(fetched as usize) {
                if p.is_null() {
                    continue;
                }
                let obj = PCWSTR(p.0);

                let values = self.properties.GetValues(obj, keys).ok();
                let content_type = values
                    .as_ref()
                    .and_then(|v| v.GetGuidValue(&WPD_OBJECT_CONTENT_TYPE).ok());

                let is_folder = matches!(
                    content_type,
                    Some(ct) if ct == WPD_CONTENT_TYPE_FOLDER || ct == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT
                );
                if is_folder {
                    self.walk(obj, keys, out)?;
                    continue;
                }

                let name = values.as_ref().and_then(|v| {
                    v.GetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME)
                        .ok()
                        .and_then(|s| pwstr_to_string_and_free(s))
                        .filter(|s| !s.is_empty())
                        .or_else(|| {
                            v.GetStringValue(&WPD_OBJECT_NAME)
                                .ok()
                                .and_then(|s| pwstr_to_string_and_free(s))
                        })
                });

                // 内容类型缺失且扩展名未知：可能是没报告类型的目录，探测其子项
                if content_type.is_none()
                    && classify(name.as_deref().unwrap_or(""), None) == FileKind::Other
                {
                    if let Ok(probe) =
                        self.content
                            .EnumObjects(0, obj, None::<&IPortableDeviceValues>)
                    {
                        let mut one = vec![PWSTR::null(); 1];
                        let mut got = 0u32;
                        let _ = probe.Next(&mut one, &mut got);
                        for q in &one {
                            if !q.is_null() {
                                CoTaskMemFree(Some(q.0 as *const core::ffi::c_void));
                            }
                        }
                        if got > 0 {
                            self.walk(obj, keys, out)?;
                            continue;
                        }
                    }
                }

                let kind = classify(name.as_deref().unwrap_or(""), content_type);
                // 只输出媒体文件；非媒体（.AAE 等）直接跳过。
                if kind == FileKind::Other {
                    continue;
                }

                let size = values
                    .as_ref()
                    .and_then(|v| v.GetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE).ok())
                    .unwrap_or(0);
                let taken_at = values.as_ref().and_then(|v| {
                    v.GetValue(&WPD_OBJECT_DATE_CREATED)
                        .ok()
                        .and_then(|pv| taken_at_from_pv(&pv))
                });
                let persistent_id = values
                    .as_ref()
                    .and_then(|v| {
                        v.GetStringValue(&WPD_OBJECT_PERSISTENT_UNIQUE_ID)
                            .ok()
                            .and_then(|s| pwstr_to_string_and_free(s))
                    })
                    .unwrap_or_default();

                out.push(MediaFile {
                    persistent_id,
                    name: name.unwrap_or_default(),
                    size,
                    taken_at,
                });
            }

            for p in &buf {
                if !p.is_null() {
                    CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
                }
            }

            if (fetched as usize) < FETCH_CHUNK {
                break;
            }
        }

        Ok(())
    }
}

/// 用稳定的 `persistent_id` 换取当前有效的对象 ID。
///
/// **为什么必须这样：** 实测（阶段 3）iOS 上从枚举拿到的对象句柄在属性读取后会失效，
/// 直接用于 `GetStream` 会拿到**错误对象**的数据（实测错位 2 个）。必须先重解析。
pub fn resolve_object_id(content: &IPortableDeviceContent, persistent_id: &str) -> Result<String> {
    unsafe {
        let col: IPortableDevicePropVariantCollection =
            CoCreateInstance(&PortableDevicePropVariantCollection, None, CLSCTX_ALL)
                .context("创建 PropVariantCollection 失败")?;
        let pv = PROPVARIANT::from(persistent_id);
        col.Add(&pv).context("加入 persistent id 失败")?;

        let out = content
            .GetObjectIDsFromPersistentUniqueIDs(&col)
            .context("GetObjectIDsFromPersistentUniqueIDs 失败")?;

        let count = 0u32;
        out.GetCount(&count).context("读取解析结果数量失败")?;
        if count == 0 {
            bail!("无法把 persistent id 解析成对象 ID: {persistent_id}");
        }

        let value = PROPVARIANT::new();
        out.GetAt(0, &value).context("读取解析结果失败")?;
        let s = BSTR::try_from(&value)
            .map(|b| b.to_string())
            .unwrap_or_default();
        if s.is_empty() {
            bail!("解析出的对象 ID 为空");
        }
        Ok(s)
    }
}

/// 设备目录名：`<model>-<serial 后6位>`（设计 §4）。型号去掉空白。
pub fn folder_name_for(model: &str, serial: Option<&str>) -> String {
    let model: String = model.chars().filter(|c| !c.is_whitespace()).collect();
    let model = if model.is_empty() {
        "Device".into()
    } else {
        model
    };
    let tail = match serial {
        Some(s) if !s.is_empty() => {
            let chars: Vec<char> = s.chars().collect();
            let start = chars.len().saturating_sub(6);
            chars[start..].iter().collect::<String>()
        }
        _ => "000000".to_string(),
    };
    format!("{model}-{tail}")
}

fn classify(name: &str, content_type: Option<GUID>) -> FileKind {
    if let Some(ct) = content_type {
        if ct == WPD_CONTENT_TYPE_IMAGE {
            return FileKind::Image;
        }
        if ct == WPD_CONTENT_TYPE_VIDEO {
            return FileKind::Video;
        }
    }
    let ext = match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    };
    match ext.as_str() {
        "heic" | "heif" | "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff"
        | "avif" => FileKind::Image,
        "mov" | "mp4" | "m4v" | "avi" | "3gp" | "mkv" => FileKind::Video,
        _ => FileKind::Other,
    }
}

unsafe fn pwstr_to_string_and_free(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
    s
}

unsafe fn key_collection(keys: &[PROPERTYKEY]) -> Result<IPortableDeviceKeyCollection> {
    let col: IPortableDeviceKeyCollection =
        CoCreateInstance(&PortableDeviceKeyCollection, None, CLSCTX_ALL)
            .context("创建 KeyCollection 失败")?;
    for k in keys {
        col.Add(k).context("KeyCollection::Add 失败")?;
    }
    Ok(col)
}

unsafe fn read_two_call(
    mut call: impl FnMut(PWSTR, *mut u32) -> windows::core::Result<()>,
) -> Option<String> {
    let mut len = 0u32;
    call(PWSTR::null(), &mut len).ok()?;
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    call(PWSTR(buf.as_mut_ptr()), &mut len).ok()?;
    Some(
        String::from_utf16_lossy(&buf)
            .trim_end_matches('\0')
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_of_epoch() {
        assert_eq!(year_of(0), 1970);
        assert_eq!(year_of(1_700_000_000), 2023);
    }

    #[test]
    fn parses_wpd_date_string() {
        let e = parse_date_string("2026/04/30:19:42:06.000").unwrap();
        assert_eq!(year_of(e), 2026);
        let expected = days_from_civil(2026, 4, 30) * 86400 + 19 * 3600 + 42 * 60 + 6;
        assert_eq!(e, expected);
    }

    #[test]
    fn parses_iso_like_and_rejects_short() {
        assert!(parse_date_string("2026-04-30 19:42:06").is_some());
        assert!(parse_date_string("not a date").is_none());
    }

    #[test]
    fn folder_name_uses_model_and_serial_tail() {
        assert_eq!(
            folder_name_for("iPhone 15 Pro", Some("F17ABC3F9A2C")),
            "iPhone15Pro-3F9A2C"
        );
        assert_eq!(folder_name_for("", None), "Device-000000");
    }

    /// 实机冒烟：需要插着 iPhone。跑：
    ///   cargo test -p liveporter real_device_transfer_smoke -- --ignored --nocapture
    #[test]
    #[ignore = "requires a connected iPhone"]
    fn real_device_transfer_smoke() {
        let _com = ComGuard::new().unwrap();
        let dev = WpdDevice::open_first().unwrap();
        let info = dev.info().clone();
        println!(
            "device={:?} model={:?} serial={:?} folder={}",
            info.friendly_name, info.model, info.serial, info.folder_name
        );

        let media = dev.list_media().unwrap();
        println!("media count = {}", media.len());

        let dir = std::env::temp_dir().join("lpm_device_smoke");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let n = std::env::var("LPM_SMOKE_N")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(30);

        let (mut ok, mut fail, mut bytes, mut wrong) = (0u32, 0u32, 0u64, 0u32);
        for (i, f) in media.iter().take(n).enumerate() {
            let dest = dir.join(format!("{i:04}_{}", f.name));
            let t0 = std::time::Instant::now();
            // 先重解析持久 ID → 新鲜对象 ID，再下载（iOS 句柄会失效）。
            let resolved = resolve_object_id(dev.content(), &f.persistent_id);
            let object_id = match resolved {
                Ok(id) => id,
                Err(e) => {
                    fail += 1;
                    println!("[{i:02}] RESOLVE-FAIL {:40} : {e:#}", f.name);
                    continue;
                }
            };
            match transfer::download_object(dev.content(), &object_id, &dest, |_| {}) {
                Ok(w) => {
                    ok += 1;
                    bytes += w;
                    let size_ok = w == f.size;
                    if !size_ok {
                        wrong += 1;
                    }
                    println!(
                        "[{i:02}] OK   {:40} reported={:>10} got={:>10} size_ok={} {:.1}s",
                        f.name,
                        f.size,
                        w,
                        size_ok,
                        t0.elapsed().as_secs_f32()
                    );
                }
                Err(e) => {
                    fail += 1;
                    println!(
                        "[{i:02}] FAIL {:40} : {e:#} ({:.1}s)",
                        f.name,
                        t0.elapsed().as_secs_f32()
                    );
                }
            }
        }
        println!("SUMMARY ok={ok} fail={fail} wrong_content={wrong} bytes={bytes}");
        assert!(ok > 0, "至少应有一个文件成功");
        assert_eq!(wrong, 0, "不应有内容/大小不符的文件");
    }

    /// 并发流实验：`LPM_PARALLEL` 个线程，各自 Open 一次设备、各下一部分文件。
    /// 跑：`cargo test -p liveporter real_device_parallel_smoke -- --ignored --nocapture`
    #[test]
    #[ignore = "requires a connected iPhone"]
    fn real_device_parallel_smoke() {
        let n: usize = std::env::var("LPM_SMOKE_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);
        let threads: usize = std::env::var("LPM_PARALLEL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        // 先用一个临时会话枚举清单（清单是纯数据，可跨线程）。
        let media = {
            let _com = ComGuard::new().unwrap();
            let dev = WpdDevice::open_first().unwrap();
            dev.list_media().unwrap()
        };
        println!("media count = {}", media.len());

        let per = n / threads;
        let dir = std::env::temp_dir().join("lpm_device_parallel");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let start = std::time::Instant::now();
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let chunk: Vec<MediaFile> = media.iter().skip(t * per).take(per).cloned().collect();
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let _com = match ComGuard::new() {
                        Ok(c) => c,
                        Err(e) => {
                            println!("thread {t} COM 初始化失败: {e:#}");
                            return (0u32, per as u32, 0u64);
                        }
                    };
                    let dev = match WpdDevice::open_first() {
                        Ok(d) => d,
                        Err(e) => {
                            println!("thread {t} Open 失败: {e:#}");
                            return (0u32, per as u32, 0u64);
                        }
                    };
                    let (mut ok, mut fail, mut bytes) = (0u32, 0u32, 0u64);
                    for (i, f) in chunk.iter().enumerate() {
                        let dest = dir.join(format!("t{t}_{i:03}_{}", f.name));
                        let oid = match resolve_object_id(dev.content(), &f.persistent_id) {
                            Ok(x) => x,
                            Err(e) => {
                                fail += 1;
                                println!("t{t} RESOLVE-FAIL {}: {e:#}", f.name);
                                continue;
                            }
                        };
                        match transfer::download_object(dev.content(), &oid, &dest, |_| {}) {
                            Ok(w) => {
                                ok += 1;
                                bytes += w;
                            }
                            Err(e) => {
                                fail += 1;
                                println!("t{t} FAIL {}: {e:#}", f.name);
                            }
                        }
                    }
                    println!("thread {t}: ok={ok} fail={fail} bytes={bytes}");
                    (ok, fail, bytes)
                })
            })
            .collect();

        let (mut ok, mut fail, mut bytes) = (0u32, 0u32, 0u64);
        for h in handles {
            let (a, b, c) = h.join().unwrap();
            ok += a;
            fail += b;
            bytes += c;
        }
        let secs = start.elapsed().as_secs_f64();
        println!(
            "PARALLEL threads={threads} ok={ok} fail={fail} bytes={bytes} secs={secs:.1} => {:.2} MB/s",
            bytes as f64 / 1048576.0 / secs.max(0.001)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 分阶段计时：定位低速/停滞到底花在哪。
    /// 跑：`cargo test -p liveporter real_device_timing_smoke -- --ignored --nocapture`
    #[test]
    #[ignore = "requires a connected iPhone"]
    fn real_device_timing_smoke() {
        use std::time::{Duration, Instant};
        use windows::Win32::Devices::PortableDevices::WPD_RESOURCE_DEFAULT;
        use windows::Win32::System::Com::{IStream, STGM_READ};

        let _com = ComGuard::new().unwrap();
        let dev = WpdDevice::open_first().unwrap();
        let media = dev.list_media().unwrap();
        let n: usize = std::env::var("LPM_SMOKE_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);
        println!("media count = {}", media.len());

        let (mut bytes_total, mut time_total) = (0u64, 0f32);
        for (i, f) in media.iter().take(n).enumerate() {
            let t_resolve = Instant::now();
            let oid = match resolve_object_id(dev.content(), &f.persistent_id) {
                Ok(x) => x,
                Err(e) => {
                    println!("[{i:02}] RESOLVE-FAIL {}: {e:#}", f.name);
                    continue;
                }
            };
            let d_resolve = t_resolve.elapsed();

            let wide: Vec<u16> = oid.encode_utf16().chain(std::iter::once(0)).collect();
            let resources = unsafe { dev.content().Transfer().unwrap() };
            let mut stream: Option<IStream> = None;
            let mut buf_size: u32 = 0;
            let mut d_get = Duration::ZERO;
            let mut attempts = 0u32;
            let mut got = false;
            for a in 1..=5u32 {
                let t = Instant::now();
                let r = unsafe {
                    resources.GetStream(
                        PCWSTR(wide.as_ptr()),
                        &WPD_RESOURCE_DEFAULT,
                        STGM_READ.0,
                        &mut buf_size,
                        &mut stream,
                    )
                };
                d_get += t.elapsed();
                attempts = a;
                if r.is_ok() && stream.is_some() {
                    got = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(150 * a as u64));
            }
            if !got {
                println!("[{i:02}] GETSTREAM-FAIL {:40}", f.name);
                continue;
            }
            let stream = stream.unwrap();
            let chunk = buf_size.clamp(64 * 1024, 1024 * 1024) as usize;
            let t_read = Instant::now();
            let mut buf = vec![0u8; chunk];
            let mut total = 0u64;
            loop {
                let mut rd = 0u32;
                let hr = unsafe {
                    stream.Read(
                        buf.as_mut_ptr() as *mut core::ffi::c_void,
                        chunk as u32,
                        Some(&mut rd),
                    )
                };
                // 已知大小：读够 f.size 就停，避免最后那次可能阻塞的 Read。
                if hr.is_err() || rd == 0 || total >= f.size {
                    break;
                }
                total += rd as u64;
            }
            let d_read = t_read.elapsed();
            drop(stream);
            drop(resources);
            std::thread::sleep(Duration::from_millis(250));

            let d_all = d_resolve + d_get + d_read;
            println!(
                "[{i:02}] {:40} sz={:>9} got={:>9} buf={:>7} resolve={:>6.0}ms get={:>5.2}s(read={} ) read={:>5.2}s total={:>5.2}s {:.2}MB/s",
                f.name,
                f.size,
                total,
                buf_size,
                d_resolve.as_secs_f32() * 1000.0,
                d_get.as_secs_f32(),
                attempts,
                d_read.as_secs_f32(),
                d_all.as_secs_f32(),
                (total as f64 / 1048576.0) / d_all.as_secs_f64().max(0.001)
            );
            bytes_total += total;
            time_total += d_all.as_secs_f32();
        }
        println!(
            "TIMING SUMMARY bytes={bytes_total} time={time_total:.1}s => {:.2} MB/s",
            (bytes_total as f64 / 1048576.0) / time_total.max(0.001) as f64
        );
    }

    /// 同一个文件连下两次，看第二次是否变快（区分缓存/取回 vs 设备固有慢）。
    #[test]
    #[ignore = "requires a connected iPhone"]
    fn real_device_repeat_smoke() {
        use std::time::{Duration, Instant};
        use windows::Win32::Devices::PortableDevices::WPD_RESOURCE_DEFAULT;
        use windows::Win32::System::Com::{IStream, STGM_READ};

        let _com = ComGuard::new().unwrap();
        let dev = WpdDevice::open_first().unwrap();
        let media = dev.list_media().unwrap();

        let movs: Vec<&MediaFile> = media
            .iter()
            .filter(|f| f.name.to_ascii_lowercase().ends_with(".mov"))
            .take(6)
            .collect();

        for f in movs {
            let t_resolve = Instant::now();
            let oid = resolve_object_id(dev.content(), &f.persistent_id).unwrap();
            let d_resolve = t_resolve.elapsed();
            let wide: Vec<u16> = oid.encode_utf16().chain(std::iter::once(0)).collect();

            for round in 1..=2 {
                let resources = unsafe { dev.content().Transfer().unwrap() };
                let mut stream: Option<IStream> = None;
                let mut buf_size = 0u32;
                let t_get = Instant::now();
                unsafe {
                    resources
                        .GetStream(
                            PCWSTR(wide.as_ptr()),
                            &WPD_RESOURCE_DEFAULT,
                            STGM_READ.0,
                            &mut buf_size,
                            &mut stream,
                        )
                        .unwrap();
                }
                let d_get = t_get.elapsed();
                let stream = stream.unwrap();
                let chunk = buf_size.clamp(64 * 1024, 1024 * 1024) as usize;
                let t_read = Instant::now();
                let mut buf = vec![0u8; chunk];
                let mut total = 0u64;
                loop {
                    let mut rd = 0u32;
                    let hr = unsafe {
                        stream.Read(
                            buf.as_mut_ptr() as *mut core::ffi::c_void,
                            chunk as u32,
                            Some(&mut rd),
                        )
                    };
                    if hr.is_err() || rd == 0 {
                        break;
                    }
                    total += rd as u64;
                }
                let d_read = t_read.elapsed();
                drop(stream);
                drop(resources);
                std::thread::sleep(Duration::from_millis(250));
                println!(
                    "{} round{round} sz={} got={} resolve={:?} get={:.2}s read={:.2}s {:.2}MB/s",
                    f.name,
                    f.size,
                    total,
                    d_resolve,
                    d_get.as_secs_f32(),
                    d_read.as_secs_f32(),
                    (total as f64 / 1048576.0) / d_read.as_secs_f64().max(0.001)
                );
            }
        }
    }

    /// 诊断 `WPD_OBJECT_DATE_CREATED` 的真实类型与转换结果。
    /// 跑：`cargo test -p liveporter real_device_date_probe -- --ignored --nocapture`
    #[test]
    #[ignore = "requires a connected iPhone"]
    fn real_device_date_probe() {
        let _com = ComGuard::new().unwrap();
        let dev = WpdDevice::open_first().unwrap();
        let media = dev.list_media().unwrap();
        let keys = unsafe { key_collection(&[WPD_OBJECT_DATE_CREATED]) }.unwrap();

        for f in media.iter().take(6) {
            let oid = match resolve_object_id(dev.content(), &f.persistent_id) {
                Ok(x) => x,
                Err(e) => {
                    println!("resolve fail {}: {e:#}", f.name);
                    continue;
                }
            };
            let wide: Vec<u16> = oid.encode_utf16().chain(std::iter::once(0)).collect();
            let values = unsafe { dev.properties.GetValues(PCWSTR(wide.as_ptr()), &keys) }.ok();
            let Some(values) = values else {
                println!("{}: GetValues 失败", f.name);
                continue;
            };
            let Some(pv) = (unsafe { values.GetValue(&WPD_OBJECT_DATE_CREATED) }).ok() else {
                println!("{}: GetValue 失败", f.name);
                continue;
            };

            // PROPVARIANT 前 2 字节是 vt (VARTYPE)
            let vt = unsafe { *(pv.as_raw() as *const _ as *const u16) };
            let as_f64 = f64::try_from(&pv).ok();
            let as_bstr = BSTR::try_from(&pv).ok().map(|b| b.to_string());
            let ours = taken_at_from_pv(&pv);
            println!(
                "{}: vt=0x{vt:04x} f64={as_f64:?} bstr={as_bstr:?} taken_at={ours:?}",
                f.name
            );
        }
    }
}
