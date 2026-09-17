//! 单文件流式下载：`IPortableDeviceResources::GetStream` + `IStream::Read`。
//!
//! **只读**：只调用 `Transfer` / `GetStream` / `Read` / `Cancel`，不写设备。

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use windows::core::PCWSTR;
use windows::Win32::Devices::PortableDevices::{IPortableDeviceContent, WPD_RESOURCE_DEFAULT};
use windows::Win32::System::Com::{IStream, STGM_READ};

/// 把设备上的一个对象流式写到本地文件。返回写入的总字节数。
///
/// `dest` 由调用方保证正确（含撞车后缀）；已存在会被覆盖。
/// `on_progress` 每读完一块回调一次（累计字节）。
pub fn download_object(
    content: &IPortableDeviceContent,
    object_id: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64),
) -> Result<u64> {
    let wide: Vec<u16> = object_id.encode_utf16().chain(std::iter::once(0)).collect();

    let resources = unsafe { content.Transfer() }.context("获取 Transfer 接口失败")?;

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

    // 设备建议值优先，夹在 [64 KiB, 1 MiB] 之间，避免过小或过大。
    if buf_size < 64 * 1024 {
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
