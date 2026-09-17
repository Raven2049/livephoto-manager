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
pub fn taken_at_from_pv(pv: &PROPVARIANT) -> Option<i64> {
    let ole = f64::try_from(pv).ok()?;
    if ole <= 0.0 {
        return None;
    }
    Some(((ole - 25569.0) * 86400.0) as i64)
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
}
