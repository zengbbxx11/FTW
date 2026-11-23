use crate::{read_frame, write_frame, Operation, Request, Response, PROTOCOL_VERSION};
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    fs::OpenOptions,
    io,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null_mut, NonNull},
};
use tokio::{
    io::AsyncWriteExt,
    net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions},
};
use windows_sys::Win32::{
    Foundation::{LocalFree, ERROR_PIPE_BUSY},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        SECURITY_IDENTIFICATION,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub struct PipeEndpoint {
    name: String,
    user_sid: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PipeOpenError {
    #[error("Core pipe is not open")]
    Absent,
    #[error("Core connection failed: {0}")]
    Io(#[source] io::Error),
}

impl PipeEndpoint {
    pub fn for_data_directory(data: &Path) -> io::Result<Self> {
        let user_sid = current_user_sid()?;
        let identity = directory_identity(data)?;
        let digest = Sha256::digest(format!("{user_sid}/{identity}"));
        Ok(Self {
            name: format!(r"\\.\pipe\ftw-{digest:x}"),
            user_sid,
        })
    }

    pub fn listen(&self) -> io::Result<NamedPipeServer> {
        let sddl: Vec<_> = format!("D:P(A;;GA;;;{})", self.user_sid)
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = null_mut();
        // Windows allocates the descriptor locally. Keep it alive until the pipe is created.
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        require_success(converted)?;
        let descriptor = LocalMemory::new(descriptor)?;
        let mut security = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0.as_ptr(),
            bInheritHandle: 0,
        };

        // This endpoint stays open for the full Core lifetime, including between clients.
        unsafe {
            ServerOptions::new()
                .first_pipe_instance(true)
                .max_instances(1)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    &self.name,
                    (&mut security as *mut SECURITY_ATTRIBUTES).cast(),
                )
        }
    }

    pub async fn health(&self) -> Result<Response, PipeOpenError> {
        self.request(&Operation::Health).await
    }

    pub async fn request(&self, operation: &Operation) -> Result<Response, PipeOpenError> {
        // Keep a pipe server from impersonating the client when it accepts a connection.
        let mut client = loop {
            let connection = ClientOptions::new()
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(&self.name);
            match connection {
                Ok(client) => break client,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(PipeOpenError::Absent);
                }
                Err(error) => return Err(PipeOpenError::Io(error)),
            }
        };

        let request = Request {
            version: PROTOCOL_VERSION,
            operation,
        };
        write_frame(&mut client, &request)
            .await
            .map_err(PipeOpenError::Io)?;
        let response = read_frame(&mut client).await.map_err(PipeOpenError::Io)?;
        client.write_u8(0).await.map_err(PipeOpenError::Io)?;
        Ok(response)
    }
}

fn directory_identity(path: &Path) -> io::Result<String> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // The file handle is valid for this call, and Windows fills the provided structure.
    require_success(unsafe {
        GetFileInformationByHandle(directory.as_raw_handle(), &mut information)
    })?;
    Ok(format!(
        "{:08x}/{:08x}{:08x}",
        information.dwVolumeSerialNumber, information.nFileIndexHigh, information.nFileIndexLow,
    ))
}

fn current_user_sid() -> io::Result<String> {
    let mut token = null_mut();
    // Open the process token with read-only access before wrapping the returned handle.
    require_success(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
    let token = unsafe { OwnedHandle::from_raw_handle(token) };

    let mut bytes = 0;
    // A zero-length query returns the required byte count for TOKEN_USER and its SID.
    unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut bytes) };
    if bytes < std::mem::size_of::<TOKEN_USER>() as u32 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
    require_success(unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    })?;

    // The returned TOKEN_USER and its SID share the aligned buffer's lifetime.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = null_mut();
    require_success(unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) })?;
    let text = LocalMemory::new(text.cast())?;
    let wide = text.0.as_ptr().cast::<u16>();
    // ConvertSidToStringSidW returns a NUL-terminated UTF-16 string.
    let length = (0..)
        .find(|&offset| unsafe { *wide.add(offset) } == 0)
        .unwrap();
    String::from_utf16(unsafe { std::slice::from_raw_parts(wide, length) })
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

struct LocalMemory(NonNull<c_void>);

impl LocalMemory {
    fn new(ptr: *mut c_void) -> io::Result<Self> {
        NonNull::new(ptr)
            .map(Self)
            .ok_or_else(|| io::Error::other("Windows returned a null allocation"))
    }
}

impl Drop for LocalMemory {
    fn drop(&mut self) {
        // The Win32 conversion functions allocate this buffer with LocalAlloc.
        unsafe { LocalFree(self.0.as_ptr()) };
    }
}

fn require_success(result: i32) -> io::Result<()> {
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
