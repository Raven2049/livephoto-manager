//! WPD PROPERTYKEY 常量。
//!
//! `windows` crate 0.58 已从 Windows SDK 自动生成并导出全部所需常量
//! （权威来源即 SDK 的 `PortableDevice.h`），因此这里直接重导出，**不手工抄录**——
//! 手工抄 GUID/PID 一旦错一位不会报编译错误，只会静默返回空值，是最难查的一类 bug。
//!
//! 若将来升级 `windows` crate 后发现某个键不再导出，再去核对头文件补声明：
//! `C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um\PortableDevice.h`

// 常量清单：当前实现只用到其中一部分，其余留给后续阶段，故允许未使用。
#[allow(unused_imports)]
pub use windows::Win32::Devices::PortableDevices::{
    WPD_DEVICE_FRIENDLY_NAME, WPD_DEVICE_MODEL, WPD_DEVICE_SERIAL_NUMBER, WPD_OBJECT_CAN_DELETE,
    WPD_OBJECT_CONTENT_TYPE, WPD_OBJECT_DATE_CREATED, WPD_OBJECT_DATE_MODIFIED, WPD_OBJECT_FORMAT,
    WPD_OBJECT_ID, WPD_OBJECT_NAME, WPD_OBJECT_ORIGINAL_FILE_NAME, WPD_OBJECT_PERSISTENT_UNIQUE_ID,
    WPD_OBJECT_SIZE, WPD_STORAGE_CAPACITY, WPD_STORAGE_FREE_SPACE_IN_BYTES,
};

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

    #[test]
    fn property_keys_are_distinct() {
        // 若两个键的 GUID+PID 完全相同，说明重导出选择有误
        let all: Vec<(&str, PROPERTYKEY)> = vec![
            ("WPD_OBJECT_ID", WPD_OBJECT_ID),
            (
                "WPD_OBJECT_PERSISTENT_UNIQUE_ID",
                WPD_OBJECT_PERSISTENT_UNIQUE_ID,
            ),
            ("WPD_OBJECT_NAME", WPD_OBJECT_NAME),
            (
                "WPD_OBJECT_ORIGINAL_FILE_NAME",
                WPD_OBJECT_ORIGINAL_FILE_NAME,
            ),
            ("WPD_OBJECT_SIZE", WPD_OBJECT_SIZE),
            ("WPD_OBJECT_DATE_CREATED", WPD_OBJECT_DATE_CREATED),
            ("WPD_OBJECT_DATE_MODIFIED", WPD_OBJECT_DATE_MODIFIED),
            ("WPD_OBJECT_FORMAT", WPD_OBJECT_FORMAT),
            ("WPD_OBJECT_CONTENT_TYPE", WPD_OBJECT_CONTENT_TYPE),
            ("WPD_OBJECT_CAN_DELETE", WPD_OBJECT_CAN_DELETE),
            ("WPD_DEVICE_FRIENDLY_NAME", WPD_DEVICE_FRIENDLY_NAME),
            ("WPD_DEVICE_MODEL", WPD_DEVICE_MODEL),
            ("WPD_DEVICE_SERIAL_NUMBER", WPD_DEVICE_SERIAL_NUMBER),
            ("WPD_STORAGE_CAPACITY", WPD_STORAGE_CAPACITY),
            (
                "WPD_STORAGE_FREE_SPACE_IN_BYTES",
                WPD_STORAGE_FREE_SPACE_IN_BYTES,
            ),
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let (na, ka) = &all[i];
                let (nb, kb) = &all[j];
                assert_ne!(
                    (ka.fmtid, ka.pid),
                    (kb.fmtid, kb.pid),
                    "{na} 与 {nb} 的 PROPERTYKEY 相同，重导出有误"
                );
            }
        }
    }

    #[test]
    fn fmtids_are_non_zero() {
        assert_ne!(WPD_OBJECT_ID.fmtid.data1, 0, "GUID 未填充");
    }
}
