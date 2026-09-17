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

    // iOS 的 PTP 会话在上一个流完全释放前，新的 GetStream 会间歇失败（实测 MOV 尤其明显，
    // 错误 0x80042007）。带退避重试若干次可以显著降低失败率。
    const MAX_ATTEMPTS: u32 = 5;
    let mut attempt = 0u32;
    let (stream, buf_size) = loop {
        attempt += 1;
        let mut buf_size: u32 = 0;
        let mut stream: Option<IStream> = None;
        let result = unsafe {
            resources.GetStream(
                PCWSTR(wide.as_ptr()),
                &WPD_RESOURCE_DEFAULT,
                STGM_READ.0,
                &mut buf_size,
                &mut stream,
            )
        };
        match result {
            Ok(()) => match stream {
                Some(s) => break (s, buf_size),
                None => {
                    if attempt >= MAX_ATTEMPTS {
                        bail!("设备未返回数据流");
                    }
                }
            },
            Err(e) => {
                if attempt >= MAX_ATTEMPTS {
                    return Err(e).context("GetStream 失败");
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(150 * attempt as u64));
    };

    // 设备建议值优先，夹在 [64 KiB, 1 MiB] 之间，避免过小或过大。
    let chunk = buf_size.clamp(64 * 1024, 1024 * 1024) as usize;

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

    // 按序显式释放流与 resources。
    //
    // 曾经在释放后加过 250ms 延迟，用来缓解 MOV 的间歇失败；但那批失败其实是 iPhone
    // 「传输到 Mac 或 PC = 自动」的即席转码造成的（见 notes/2026-09-17-import-smoke.md）。
    // 设为「保留原件」后，延迟已无必要：实测去掉延迟并保持并发 1，吞吐从 ~1.65 提升到
    // ~26 MB/s 且零失败。故**不要**再加回延迟。
    drop(stream);
    drop(resources);

    // 也**不要**在成功路径调用 `resources.Cancel()`：实测既无帮助又会干扰会话。
    // Cancel 只应用于真正的用户取消场景。
    Ok(total)
}
