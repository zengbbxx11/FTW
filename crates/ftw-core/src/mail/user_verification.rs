use std::{
    os::windows::io::{AsHandle, FromRawHandle, OwnedHandle},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{ERROR_CANCELLED, ERROR_SUCCESS},
    Security::{
        Credentials::{
            CredUIPromptForCredentialsW, CREDUI_FLAGS_ALWAYS_SHOW_UI, CREDUI_FLAGS_DO_NOT_PERSIST,
            CREDUI_FLAGS_EXCLUDE_CERTIFICATES, CREDUI_FLAGS_GENERIC_CREDENTIALS, CREDUI_INFOW,
        },
        LogonUserW, LOGON32_LOGON_NETWORK, LOGON32_PROVIDER_DEFAULT,
    },
};

pub fn verify(message: &str) -> Result<(), &'static str> {
    let caption = wide("FTW · 验证 Windows 用户");
    let message = wide(message);
    let target = wide("FTW Agent mail permission");
    let info = CREDUI_INFOW {
        cbSize: std::mem::size_of::<CREDUI_INFOW>() as u32,
        hwndParent: null_mut(),
        pszMessageText: message.as_ptr(),
        pszCaptionText: caption.as_ptr(),
        hbmBanner: null_mut(),
    };
    let mut user = [0u16; 514];
    let mut password = [0u16; 257];
    let mut save = 0;
    let result = unsafe {
        CredUIPromptForCredentialsW(
            &info,
            target.as_ptr(),
            null(),
            0,
            user.as_mut_ptr(),
            user.len() as u32,
            password.as_mut_ptr(),
            password.len() as u32,
            &mut save,
            CREDUI_FLAGS_ALWAYS_SHOW_UI
                | CREDUI_FLAGS_DO_NOT_PERSIST
                | CREDUI_FLAGS_EXCLUDE_CERTIFICATES
                | CREDUI_FLAGS_GENERIC_CREDENTIALS,
        )
    };
    let result = match result {
        ERROR_SUCCESS => authenticate(&user, &password),
        ERROR_CANCELLED => Err("agent_policy_cancelled"),
        _ => Err("agent_policy_verification_failed"),
    };
    for value in &mut password {
        // Clear the password before the stack buffer is released.
        unsafe { std::ptr::write_volatile(value, 0) };
    }
    result
}

fn authenticate(user: &[u16], password: &[u16]) -> Result<(), &'static str> {
    let error = "agent_policy_verification_failed";
    if user[0] == 0 || password[0] == 0 {
        return Err(error);
    }
    let account = &user[..user.iter().position(|x| *x == 0).ok_or(error)?];
    let name = String::from_utf16(account).map_err(|_| error)?;
    let (domain, name) = match name.split_once('\\') {
        Some((domain, name)) => (Some(wide(domain)), wide(name)),
        None if name.contains('@') => (None, wide(&name)),
        None => (Some(wide(".")), wide(&name)),
    };
    let mut logon = null_mut();
    let valid = unsafe {
        LogonUserW(
            name.as_ptr(),
            domain.as_ref().map_or(null(), |s| s.as_ptr()),
            password.as_ptr(),
            LOGON32_LOGON_NETWORK,
            LOGON32_PROVIDER_DEFAULT,
            &mut logon,
        )
    };
    if valid == 0 {
        return Err(error);
    }
    let token = unsafe { OwnedHandle::from_raw_handle(logon) };
    let authenticated = ftw_ipc::token_user_sid(token.as_handle()).map_err(|_| error)?;
    let current = ftw_ipc::current_user_sid().map_err(|_| error)?;
    if authenticated == current {
        Ok(())
    } else {
        Err(error)
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
