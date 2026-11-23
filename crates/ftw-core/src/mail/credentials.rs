use std::{io, ptr};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};

pub fn protect(password: &str) -> io::Result<Vec<u8>> {
    transform(password.as_bytes(), true)
}

pub fn read(protected: &[u8]) -> io::Result<String> {
    let plaintext = transform(protected, false)?;
    String::from_utf8(plaintext).map_err(|_| io::Error::other("invalid credential encoding"))
}

fn transform(bytes: &[u8], encrypt: bool) -> io::Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len())
            .map_err(|_| io::Error::other("credential is too large"))?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut result = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // DPAPI reads the borrowed bytes and allocates a user-scoped result with LocalAlloc.
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut result,
            )
        } else {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut result,
            )
        }
    };
    if success == 0 || result.pbData.is_null() {
        return Err(io::Error::last_os_error());
    }
    // Copy the result into owned Rust memory, then wipe and release the native allocation.
    unsafe {
        let data = std::slice::from_raw_parts_mut(result.pbData, result.cbData as usize);
        let owned = data.to_vec();
        for byte in data {
            std::ptr::write_volatile(byte, 0);
        }
        LocalFree(result.pbData.cast());
        Ok(owned)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_dpapi_round_trip() {
        let fixture = "local-dpapi-test-value";
        let protected = super::protect(fixture).unwrap();
        assert!(!protected
            .windows(fixture.len())
            .any(|part| part == fixture.as_bytes()));
        assert_eq!(super::read(&protected).unwrap(), fixture);
    }
}
