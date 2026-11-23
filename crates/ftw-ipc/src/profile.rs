use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    Portable,
    LocalAppData,
    Development,
}

pub struct Profile {
    pub mode: StorageMode,
    pub root: PathBuf,
    pub data: PathBuf,
}

impl Profile {
    pub fn resolve(executable: &Path, development_root: Option<PathBuf>) -> io::Result<Self> {
        let application_dir = executable
            .parent()
            .ok_or_else(|| io::Error::other("application directory is unavailable"))?;
        let (mode, root) = match development_root {
            Some(path) => (StorageMode::Development, path),
            None if application_dir.join("portable.flag").is_file() => {
                (StorageMode::Portable, application_dir.to_owned())
            }
            None => {
                let local = env::var_os("LOCALAPPDATA")
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| io::Error::other("LOCALAPPDATA is unavailable"))?;
                (StorageMode::LocalAppData, PathBuf::from(local).join("FTW"))
            }
        };

        if !root.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "data root must be absolute",
            ));
        }

        let data = root.join("data");
        fs::create_dir_all(&data).map_err(|error| path_error("create", &data, error))?;

        Ok(Self {
            mode,
            root: fs::canonicalize(&root).map_err(|error| path_error("resolve", &root, error))?,
            data: fs::canonicalize(&data).map_err(|error| path_error("resolve", &data, error))?,
        })
    }
}

fn path_error(action: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("could not {action} {}: {error}", display_path(path)),
    )
}

pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}");
    }

    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}
