//! 真实 WPD 实现：实现 `DeviceSource`。
//!
//! **只读约束：** 本文件及整个 `wpd/` 目录只允许读操作。任何删除 / 移动 /
//! 复制 / 新建对象的调用都会破坏"探测不写设备"的承诺。`main.rs` 里有
//! `readonly_guard` 测试看守这条规则。

use anyhow::{bail, Context, Result};
use windows::core::{BSTR, GUID, PCWSTR, PROPVARIANT, PWSTR};
use windows::Win32::Devices::PortableDevices::{
    IEnumPortableDeviceObjectIDs, IPortableDevice, IPortableDeviceContent,
    IPortableDeviceKeyCollection, IPortableDeviceManager, IPortableDeviceProperties,
    IPortableDeviceValues, PortableDevice, PortableDeviceFTM, PortableDeviceKeyCollection,
    PortableDeviceManager, PortableDeviceValues, WPD_CONTENT_TYPE_FOLDER,
    WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT, WPD_CONTENT_TYPE_IMAGE, WPD_CONTENT_TYPE_VIDEO,
    WPD_DEVICE_OBJECT_ID, WPD_FUNCTIONAL_CATEGORY_STORAGE, WPD_STORAGE_CAPACITY,
    WPD_STORAGE_FREE_SPACE_IN_BYTES,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

use crate::model::{mask_serial, FileKind, RemoteFile, StorageInfo};
use crate::source::{DeviceInfo, DeviceSource};
use crate::wpd::keys::*;

/// RAII：初始化/反初始化 COM。
pub struct ComGuard;

impl ComGuard {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .context("CoInitializeEx 失败")?;
        }
        Ok(ComGuard)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// 把可能缺失的属性组合成 `RemoteFile`。缺失字段用 `None`/默认值，绝不 panic。
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

/// 根据内容类型判定图片/视频，缺失时回退到扩展名。
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
        "heic" | "heif" | "jpg" | "jpeg" | "png" | "gif" | "tif" | "tiff" | "dng" | "bmp"
        | "webp" => FileKind::Image,
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

const FETCH_CHUNK: usize = 128;

pub struct WpdDeviceSource {
    manager: IPortableDeviceManager,
    device: IPortableDevice,
    content: IPortableDeviceContent,
    properties: IPortableDeviceProperties,
    pnp_id: Vec<u16>,
}

impl WpdDeviceSource {
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

            // 设备列表里可能混有其它 WPD 设备（实测遇到过把本机磁盘映射成 WPD 的设备），
            // 而对这类设备调用 `Open` 会**无限期挂起**。因此只打开友好名明确是
            // Apple/iPhone 的设备；没有匹配就当作"无设备"优雅退出。
            let mut apple_candidates: Vec<Vec<u16>> = Vec::new();
            let mut other_count = 0usize;
            for c in candidates {
                let name = Self::read_two_call(|p, l| {
                    manager.GetDeviceFriendlyName(PCWSTR(c.as_ptr()), p, l)
                });
                match name {
                    Some(n)
                        if {
                            let l = n.to_ascii_lowercase();
                            l.contains("iphone") || l.contains("apple")
                        } =>
                    {
                        apple_candidates.push(c)
                    }
                    _ => other_count += 1,
                }
            }

            if apple_candidates.is_empty() {
                if other_count > 0 {
                    bail!(
                        "未发现 iPhone 设备（检测到 {other_count} 个非 Apple 的 WPD 设备，已跳过）：请插上手机、解锁、并在手机上点\"信任此电脑\""
                    );
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
                for cand in &apple_candidates {
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
                    apple_candidates.len(),
                    last_err
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "无设备".into())
                )
            })?;

            let content = device.Content().context("获取设备内容失败")?;
            let properties = content.Properties().context("获取属性接口失败")?;

            Ok(WpdDeviceSource {
                manager,
                device,
                content,
                properties,
                pnp_id,
            })
        }
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

    unsafe fn walk(
        &self,
        parent: PCWSTR,
        keys: &IPortableDeviceKeyCollection,
        out: &mut Vec<RemoteFile>,
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
                let object_id = p.to_string().unwrap_or_default();
                let obj_pcwstr = PCWSTR(p.0);

                let values = self.properties.GetValues(obj_pcwstr, keys).ok();

                let content_type = values
                    .as_ref()
                    .and_then(|v| v.GetGuidValue(&WPD_OBJECT_CONTENT_TYPE).ok());

                let is_folder = matches!(
                    content_type,
                    Some(ct)
                        if ct == WPD_CONTENT_TYPE_FOLDER
                            || ct == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT
                );

                if is_folder {
                    self.walk(obj_pcwstr, keys, out)?;
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
                            .EnumObjects(0, obj_pcwstr, None::<&IPortableDeviceValues>)
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
                            self.walk(obj_pcwstr, keys, out)?;
                            continue;
                        }
                    }
                }

                let size = values
                    .as_ref()
                    .and_then(|v| v.GetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE).ok());
                let date_created = values.as_ref().and_then(|v| {
                    v.GetValue(&WPD_OBJECT_DATE_CREATED)
                        .ok()
                        .map(|pv| pv.to_string())
                        .filter(|s| !s.is_empty())
                });
                let kind = classify(name.as_deref().unwrap_or(""), content_type);

                out.push(build_remote_file(object_id, name, size, date_created, kind));
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

impl DeviceSource for WpdDeviceSource {
    fn device_info(&self) -> Result<DeviceInfo> {
        unsafe {
            let id = PCWSTR(self.pnp_id.as_ptr());
            let friendly_name =
                Self::read_two_call(|p, l| self.manager.GetDeviceFriendlyName(id, p, l));
            let model = Self::read_two_call(|p, l| self.manager.GetDeviceDescription(id, p, l));

            let keys = key_collection(&[WPD_DEVICE_SERIAL_NUMBER])?;
            let serial = self
                .content
                .Properties()?
                .GetValues(WPD_DEVICE_OBJECT_ID, &keys)
                .ok()
                .and_then(|v| v.GetStringValue(&WPD_DEVICE_SERIAL_NUMBER).ok())
                .and_then(|s| pwstr_to_string_and_free(s))
                .map(|s| mask_serial(&s));

            Ok(DeviceInfo {
                friendly_name,
                model,
                serial,
            })
        }
    }

    fn storages(&self) -> Result<Vec<StorageInfo>> {
        unsafe {
            let caps = self.device.Capabilities().context("获取设备能力失败")?;
            let functional = caps
                .GetFunctionalObjects(&WPD_FUNCTIONAL_CATEGORY_STORAGE)
                .context("获取存储功能对象失败")?;

            let count = 0u32;
            functional.GetCount(&count).context("读取存储数量失败")?;

            let keys = key_collection(&[
                WPD_OBJECT_NAME,
                WPD_STORAGE_CAPACITY,
                WPD_STORAGE_FREE_SPACE_IN_BYTES,
            ])?;

            let mut out = Vec::new();
            for i in 0..count {
                let value = PROPVARIANT::new();
                if functional.GetAt(i, &value).is_err() {
                    continue;
                }
                let object_id = BSTR::try_from(&value)
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                if object_id.is_empty() {
                    continue;
                }

                let wide: Vec<u16> = object_id.encode_utf16().chain(std::iter::once(0)).collect();
                let values = self
                    .content
                    .Properties()?
                    .GetValues(PCWSTR(wide.as_ptr()), &keys)
                    .ok();

                let name = values
                    .as_ref()
                    .and_then(|v| v.GetStringValue(&WPD_OBJECT_NAME).ok())
                    .and_then(|s| pwstr_to_string_and_free(s))
                    .unwrap_or_default();
                let capacity = values
                    .as_ref()
                    .and_then(|v| v.GetUnsignedLargeIntegerValue(&WPD_STORAGE_CAPACITY).ok());
                let free_space = values.as_ref().and_then(|v| {
                    v.GetUnsignedLargeIntegerValue(&WPD_STORAGE_FREE_SPACE_IN_BYTES)
                        .ok()
                });

                out.push(StorageInfo {
                    object_id,
                    name,
                    capacity,
                    free_space,
                });
            }

            Ok(out)
        }
    }

    fn list_media(&self) -> Result<Vec<RemoteFile>> {
        unsafe {
            let keys = key_collection(&[
                WPD_OBJECT_NAME,
                WPD_OBJECT_ORIGINAL_FILE_NAME,
                WPD_OBJECT_SIZE,
                WPD_OBJECT_DATE_CREATED,
                WPD_OBJECT_CONTENT_TYPE,
            ])?;
            let mut out = Vec::new();
            self.walk(WPD_DEVICE_OBJECT_ID, &keys, &mut out)?;
            Ok(out)
        }
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

    #[test]
    fn classify_prefers_content_type_over_extension() {
        assert_eq!(
            classify("odd.name.bin", Some(WPD_CONTENT_TYPE_IMAGE)),
            FileKind::Image
        );
        assert_eq!(
            classify("movie.dat", Some(WPD_CONTENT_TYPE_VIDEO)),
            FileKind::Video
        );
    }

    #[test]
    fn classify_falls_back_to_extension() {
        assert_eq!(classify("IMG_0001.MOV", None), FileKind::Video);
        assert_eq!(classify("IMG_0001.HEIC", None), FileKind::Image);
        assert_eq!(classify("IMG_0001.AAE", None), FileKind::Other);
        assert_eq!(classify("noext", None), FileKind::Other);
    }
}
