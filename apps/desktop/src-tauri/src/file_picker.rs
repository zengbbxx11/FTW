use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER,
    OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

pub fn choose(owner: isize) -> std::io::Result<Vec<String>> {
    let filter: Vec<_> = "PDF、JPEG、PNG、WebP\0*.pdf;*.jpg;*.jpeg;*.png;*.webp\0\0"
        .encode_utf16()
        .collect();
    let mut buffer = [0; 32768];
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner as _,
        lpstrFilter: filter.as_ptr(),
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR | OFN_ALLOWMULTISELECT,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        return result_on_cancel(Vec::new());
    }
    let paths: Vec<_> = buffer
        .split(|c| *c == 0)
        .take_while(|s| !s.is_empty())
        .map(OsString::from_wide)
        .collect();
    let (path, names) = paths
        .split_first()
        .ok_or_else(|| std::io::Error::other("missing selected file"))?;
    let path = PathBuf::from(path);
    if names.is_empty() {
        Ok(vec![path.to_string_lossy().into_owned()])
    } else {
        Ok(names
            .iter()
            .map(|name| path.join(name).to_string_lossy().into_owned())
            .collect())
    }
}

pub fn save(owner: isize, filename: &str) -> std::io::Result<Option<String>> {
    let name: Vec<_> = filename.encode_utf16().take(220).collect();
    let mut buffer = [0; 32768];
    buffer[..name.len()].copy_from_slice(&name);
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner as _,
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        Flags: OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT,
        ..Default::default()
    };
    if unsafe { GetSaveFileNameW(&mut dialog) } == 0 {
        return result_on_cancel(None);
    }
    let end = buffer
        .iter()
        .position(|code| *code == 0)
        .unwrap_or(buffer.len());
    Ok(Some(
        OsString::from_wide(&buffer[..end])
            .to_string_lossy()
            .into_owned(),
    ))
}

fn result_on_cancel<T>(cancelled: T) -> std::io::Result<T> {
    let error = unsafe { CommDlgExtendedError() };
    if error == 0 {
        Ok(cancelled)
    } else {
        Err(std::io::Error::other(format!(
            "Windows file dialog error {error}"
        )))
    }
}
