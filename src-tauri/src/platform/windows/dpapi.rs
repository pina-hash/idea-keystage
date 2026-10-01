//! DPAPI, current-user scope: `CryptProtectData` and `CryptUnprotectData`.
//!
//! Sources (read 2026-10-01): CryptProtectData ("Typically, only a user with
//! the same logon credential as the user who encrypted the data can decrypt
//! the data"; `CRYPTPROTECT_LOCAL_MACHINE` is left unset; outputs are freed
//! with `LocalFree`; the prompt struct is deprecated, so it is `None` and
//! `CRYPTPROTECT_UI_FORBIDDEN` is set) and CryptUnprotectData ("clear it
//! from memory by calling the SecureZeroMemory function"). The base64
//! framing and the validation of decrypted bytes are in
//! `platform/secret.rs`.

use std::ffi::c_void;

use windows::core::w;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

use super::super::secret::wipe;
use super::super::PlatformError;

/// Application entropy: a blob made by another program for this user does
/// not decrypt here without it. Not a secret.
const ENTROPY: &[u8] = b"IDEA Keystage/obs-websocket/v1";

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    }
}

/// Copies a DPAPI output blob, optionally wipes it, and frees it.
fn take_output(out: &mut CRYPT_INTEGER_BLOB, wipe_after: bool) -> Vec<u8> {
    if out.pbData.is_null() {
        return Vec::new();
    }
    let len = out.cbData as usize;
    // SAFETY: DPAPI allocated `cbData` bytes at `pbData`.
    let bytes = unsafe { std::slice::from_raw_parts_mut(out.pbData, len) };
    let copy = bytes.to_vec();
    if wipe_after {
        wipe(bytes);
    }
    // SAFETY: "free its pbData member by calling the LocalFree function".
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut c_void)));
    }
    out.pbData = std::ptr::null_mut();
    out.cbData = 0;
    copy
}

/// Encrypts bytes for the current user.
pub fn protect(plain: &[u8]) -> Result<Vec<u8>, PlatformError> {
    if u32::try_from(plain.len()).is_err() {
        return Err(PlatformError::InvalidInput("the secret is too long"));
    }
    let input = blob(plain);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input and entropy point at live slices; `out` receives a
    // LocalAlloc'd buffer that `take_output` frees.
    unsafe {
        CryptProtectData(
            &input,
            w!("IDEA Keystage"),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    .map_err(|e| PlatformError::os("CryptProtectData", &e))?;
    Ok(take_output(&mut out, false))
}

/// Decrypts a blob made by [`protect`] for the current user. The DPAPI
/// output buffer is wiped before it is freed; the caller wipes the copy.
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, PlatformError> {
    if u32::try_from(data.len()).is_err() {
        return Err(PlatformError::InvalidData);
    }
    let input = blob(data);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as in `protect`.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    // Tampering, another user's blob or garbage: "Applications should not
    // rely on a specific error code to detect data tampering."
    .map_err(|_| PlatformError::InvalidData)?;
    Ok(take_output(&mut out, true))
}
