mod model;
mod pairing;
mod report;
mod source;

#[cfg(windows)]
mod wpd;

#[cfg(windows)]
fn run() -> anyhow::Result<i32> {
    use crate::source::DeviceSource;
    use anyhow::Context;

    struct Args {
        json: bool,
        out: Option<String>,
    }

    fn parse_args() -> Args {
        let mut json = false;
        let mut out = None;
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--json" => json = true,
                "--out" => out = it.next(),
                _ => {}
            }
        }
        Args { json, out }
    }

    let args = parse_args();

    let _com = wpd::device::ComGuard::new()?;

    let source = match wpd::device::WpdDeviceSource::open_first() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e:#}");
            return Ok(2);
        }
    };

    let info = source.device_info().context("读取设备信息失败")?;
    let storages = source.storages().unwrap_or_default();
    let files = source.list_media().context("枚举媒体文件失败")?;
    let pairs = pairing::pair(&files);
    let report = report::build_report(info.friendly_name, info.model, info.serial, &files, &pairs);

    let text: String = if args.json {
        serde_json::to_string_pretty(&report)?
    } else {
        let mut t = String::new();
        for s in &storages {
            t.push_str(&format!(
                "存储: {}  容量={}  可用={}\n",
                s.name,
                s.capacity
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".into()),
                s.free_space
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".into())
            ));
        }
        t.push_str(&report::render_human(&report));
        t
    };

    match args.out {
        Some(path) => std::fs::write(&path, &text).with_context(|| format!("写入 {path} 失败"))?,
        None => println!("{text}"),
    }

    Ok(if report.go { 0 } else { 2 })
}

fn main() {
    #[cfg(windows)]
    {
        match run() {
            Ok(code) => std::process::exit(code),
            Err(e) => {
                eprintln!("错误: {e:#}");
                std::process::exit(1);
            }
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!("probe 仅支持 Windows");
        std::process::exit(1);
    }
}

/// 守卫测试：`wpd/` 目录下不得出现对设备的写操作。
///
/// 与计划文档给的 needle 列表相比略有调整：计划用裸词 `"Delete"`，但那会误伤
/// `keys.rs` 里重导出的 `WPD_OBJECT_CAN_DELETE`。这里改用"方法调用"形态
/// （`".Delete("` 等），既不会误报，也能挡住实际写操作。
#[cfg(test)]
mod readonly_guard {
    #[test]
    fn wpd_module_contains_no_write_operations() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/wpd");
        let mut offenders = Vec::new();
        for entry in walkdir(dir) {
            let text = std::fs::read_to_string(&entry).unwrap();
            for needle in [".Delete(", ".Move(", ".Copy(", "CreateObject", "CopyHere"] {
                if text.contains(needle) {
                    offenders.push(format!("{entry}: {needle}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "探测程序必须是只读的，发现写操作: {offenders:?}"
        );
    }

    fn walkdir(dir: &str) -> Vec<String> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walkdir(p.to_str().unwrap()));
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p.to_str().unwrap().to_string());
            }
        }
        out
    }
}
