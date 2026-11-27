use crate::{Profile, StorageMode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::System::{
        Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ},
        SystemInformation::OSVERSIONINFOW,
    },
};

const LOG_LIMIT: u64 = 256 * 1024;
const EVENTS_PER_COMPONENT: usize = 200;
const COMPONENTS: [Component; 3] = [Component::Core, Component::Desktop, Component::Mcp];

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Core,
    Desktop,
    Mcp,
}

impl Component {
    fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Desktop => "desktop",
            Self::Mcp => "mcp",
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    unix_ms: u64,
    pid: u32,
    component: Component,
    operation: String,
    code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_ref: Option<String>,
}

#[derive(Clone)]
pub struct Log {
    component: Component,
    locations: Vec<PathBuf>,
}

impl Log {
    pub fn new(executable: &Path, dev_root: Option<PathBuf>, component: Component) -> Self {
        let location = Profile::location(executable, dev_root).ok();
        Self {
            component,
            locations: log_locations(
                location.as_ref().map(|(_, root)| root.as_path()),
                executable,
            ),
        }
    }

    pub fn event(&self, operation: &str, code: &str) {
        self.record(operation, code, None, None, None);
    }

    pub fn install_panic_hook(&self) {
        let log = self.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log.event("process", "rust_panic");
            previous(info);
        }));
    }

    pub fn ready(&self, schema_version: i64) {
        self.record("startup", "ready", Some(schema_version), None, None);
    }

    pub fn mail_result(&self, operation: &str, code: &str, job_id: Option<&str>) {
        self.record(
            operation,
            code,
            None,
            job_id.filter(|id| job_identifier(id)),
            None,
        );
    }

    pub fn mail_task_result(
        &self,
        operation: &str,
        code: &str,
        job_id: Option<&str>,
        task_id: Option<&str>,
    ) {
        let task_ref = task_id
            .filter(|id| !id.is_empty() && id.len() <= 120)
            .map(|id| format!("{:x}", Sha256::digest(id.as_bytes()))[..16].to_owned());
        self.record(
            operation,
            code,
            None,
            job_id.filter(|id| job_identifier(id)),
            task_ref,
        );
    }

    fn record(
        &self,
        operation: &str,
        code: &str,
        schema_version: Option<i64>,
        job_id: Option<&str>,
        task_ref: Option<String>,
    ) {
        if !label(operation) || !label(code) {
            return;
        }
        let event = Event {
            unix_ms: now(),
            pid: process::id(),
            component: self.component,
            operation: operation.to_owned(),
            code: code.to_owned(),
            schema_version,
            job_id: job_id.map(str::to_owned),
            task_ref,
        };
        if let Ok(mut line) = serde_json::to_vec(&event) {
            line.push(b'\n');
            for location in &self.locations {
                if append(location, self.component, &line).is_ok() {
                    break;
                }
            }
        }
    }
}

fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn job_identifier(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, b)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn log_locations(root: Option<&Path>, executable: &Path) -> Vec<PathBuf> {
    let mut locations: Vec<_> = root.into_iter().map(|root| root.join("logs")).collect();
    if let Some(local) = env::var_os("LOCALAPPDATA").filter(|path| !path.is_empty()) {
        let key = root.or_else(|| executable.parent()).unwrap_or(executable);
        // Use the same fallback for the three executables without disclosing the original path.
        let key = format!(
            "{:x}",
            Sha256::digest(key.to_string_lossy().to_lowercase().as_bytes())
        );
        locations.push(
            PathBuf::from(local)
                .join("FTW")
                .join("diagnostics-fallback")
                .join(&key[..24]),
        );
    }
    locations
}

fn files(directory: &Path, component: Component) -> (PathBuf, PathBuf) {
    let name = component.name();
    (
        directory.join(format!("{name}-diagnostic.jsonl")),
        directory.join(format!("{name}-diagnostic.previous.jsonl")),
    )
}

fn append(directory: &Path, component: Component, line: &[u8]) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let _guard = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(directory.join(format!("{}-diagnostic.lock", component.name())))?;
    let (current, previous) = files(directory, component);
    if fs::metadata(&current).is_ok_and(|info| info.len() + line.len() as u64 > LOG_LIMIT) {
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&current, &previous)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(current)?
        .write_all(line)
}

fn read_events(path: &Path, component: Component, events: &mut Vec<Event>) {
    let Ok(mut file) = File::open(path) else {
        return;
    };
    let mut bytes = Vec::new();
    let read = (|| -> io::Result<()> {
        let start = file.metadata()?.len().saturating_sub(LOG_LIMIT);
        file.seek(SeekFrom::Start(start))?;
        file.take(LOG_LIMIT).read_to_end(&mut bytes)?;
        Ok(())
    })();
    if read.is_err() {
        return;
    }
    events.extend(bytes.split(|byte| *byte == b'\n').filter_map(|line| {
        let event: Event = serde_json::from_slice(line).ok()?;
        (event.component == component
            && label(&event.operation)
            && label(&event.code)
            && event.job_id.as_deref().is_none_or(job_identifier)
            && event.task_ref.as_deref().is_none_or(|value| {
                value.len() == 16 && value.bytes().all(|b| b.is_ascii_hexdigit())
            }))
        .then_some(event)
    }));
}

#[derive(Serialize)]
struct Report {
    format_version: u8,
    generated_unix_ms: u64,
    ftw_version: &'static str,
    architecture: &'static str,
    windows_version: Option<String>,
    webview2_runtime_version: Option<String>,
    storage_mode: Option<StorageMode>,
    last_observed_schema_version: Option<i64>,
    binaries: Vec<Binary>,
    events: Vec<Event>,
}

#[derive(Serialize)]
struct Binary {
    name: &'static str,
    present: bool,
    sha256: Option<String>,
}

pub fn report(executable: &Path, dev_root: Option<PathBuf>) -> io::Result<String> {
    let location = Profile::location(executable, dev_root).ok();
    let locations = log_locations(
        location.as_ref().map(|(_, root)| root.as_path()),
        executable,
    );
    let mut events = Vec::new();
    let mut schema = None;
    for component in COMPONENTS {
        let mut recent = Vec::new();
        for directory in &locations {
            let (current, previous) = files(directory, component);
            read_events(&previous, component, &mut recent);
            read_events(&current, component, &mut recent);
        }
        recent.sort_by_key(|event| event.unix_ms);
        if component == Component::Core {
            schema = recent.iter().rev().find_map(|event| event.schema_version);
        }
        let keep = recent.len().saturating_sub(EVENTS_PER_COMPONENT);
        events.extend(recent.into_iter().skip(keep));
    }
    events.sort_by_key(|event| event.unix_ms);
    let binaries = ["workbench.exe", "ftw-core.exe", "ftw-mcp.exe"]
        .into_iter()
        .map(|name| {
            let path = executable.with_file_name(name);
            Binary {
                name,
                present: path.is_file(),
                sha256: binary_hash(&path),
            }
        })
        .collect();
    Ok(serde_json::to_string_pretty(&Report {
        format_version: 2,
        generated_unix_ms: now(),
        ftw_version: env!("CARGO_PKG_VERSION"),
        architecture: env::consts::ARCH,
        windows_version: windows_version(),
        webview2_runtime_version: webview2_version(),
        storage_mode: location.map(|(mode, _)| mode),
        last_observed_schema_version: schema,
        binaries,
        events,
    })?)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn binary_hash(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > 128 * 1024 * 1024 {
        return None;
    }
    let mut hash = Sha256::new();
    io::copy(&mut file, &mut hash).ok()?;
    Some(format!("{:x}", hash.finalize()))
}

fn windows_version() -> Option<String> {
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    (unsafe { RtlGetVersion(&mut version) } == 0).then(|| {
        format!(
            "{}.{}.{}",
            version.dwMajorVersion, version.dwMinorVersion, version.dwBuildNumber
        )
    })
}

fn webview2_version() -> Option<String> {
    let client = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    version_value(HKEY_CURRENT_USER, &format!(r"SOFTWARE\{client}")).or_else(|| {
        version_value(
            HKEY_LOCAL_MACHINE,
            &format!(r"SOFTWARE\WOW6432Node\{client}"),
        )
    })
}

fn version_value(hive: HKEY, key: &str) -> Option<String> {
    let key: Vec<_> = key.encode_utf16().chain(Some(0)).collect();
    let mut buffer = [0u16; 64];
    let mut bytes = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            hive,
            key.as_ptr(),
            [112, 118, 0].as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != 0 {
        return None;
    }
    let version = String::from_utf16(&buffer[..buffer.iter().position(|unit| *unit == 0)?]).ok()?;
    let parts = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (parts.len() == 4 && parts.iter().any(|part| *part != 0)).then_some(version)
}
