use crate::model::{RemoteFile, StorageInfo};

/// 设备访问的抽象边界。
///
/// 纯逻辑（配对、报告）只依赖这个 trait，不依赖任何 Win32 类型，
/// 因此可以用假实现完整单元测试，也便于将来接入"本地备份解析"等替代通道。
pub trait DeviceSource {
    /// 设备友好名、型号、序列号
    fn device_info(&self) -> anyhow::Result<DeviceInfo>;

    /// 设备上的所有存储卷
    fn storages(&self) -> anyhow::Result<Vec<StorageInfo>>;

    /// 遍历相册目录，返回所有媒体文件
    fn list_media(&self) -> anyhow::Result<Vec<RemoteFile>>;
}

#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub friendly_name: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
}
