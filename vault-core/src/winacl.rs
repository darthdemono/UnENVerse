// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Owner-only access on Windows: a protected DACL holding one entry, the current user.
//!
//! Windows has no mode bits. A file inherits the access list of its folder, which in a profile
//! usually lets other local accounts (and Administrators, and SYSTEM) read it. This replaces that
//! list with a single allow entry for the user running the process and marks it protected, so
//! nothing is inherited back in. On a directory the entry is inherited by what is created inside it.
//!
//! This is an owner-only list, not protection from an administrator: an administrator can take
//! ownership of any file. It does keep other ordinary accounts and group-wide grants out.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::Security::Authorization::{SetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    AddAccessAllowedAceEx, GetLengthSid, GetTokenInformation, InitializeAcl, TokenUser, ACL,
    ACL_REVISION, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE,
    PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The current user's token information, as a buffer that holds a `TOKEN_USER` (the SID it points
/// to lives in the same buffer).
fn token_user() -> Result<Vec<u64>, String> {
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage (Win32 token query; every pointer is a local buffer sized by the same call)
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(format!("OpenProcessToken failed ({})", GetLastError()));
        }
        let mut needed = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        // u64 elements keep the buffer 8-byte aligned, which TOKEN_USER requires.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr().cast::<c_void>(),
            (buf.len() * 8) as u32,
            &mut needed,
        );
        let err = GetLastError();
        CloseHandle(token);
        if ok == 0 {
            return Err(format!("GetTokenInformation failed ({err})"));
        }
        Ok(buf)
    }
}

/// Replaces `path`'s access list with a protected one that allows only the current user.
pub fn restrict_to_owner(path: &Path, directory: bool) -> Result<(), String> {
    let user = token_user()?;
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage (builds one ACL in a local buffer and hands it to SetNamedSecurityInfoW)
    unsafe {
        let sid = (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        // Header (8) + one allow entry (up to 8 + SID) rounded up, with room to spare.
        let mut acl_buf = vec![0u32; 64 + GetLengthSid(sid) as usize / 4];
        let acl = acl_buf.as_mut_ptr().cast::<ACL>();
        if InitializeAcl(acl, (acl_buf.len() * 4) as u32, ACL_REVISION) == 0 {
            return Err(format!("InitializeAcl failed ({})", GetLastError()));
        }
        let flags = if directory {
            CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE
        } else {
            0
        };
        if AddAccessAllowedAceEx(acl, ACL_REVISION, flags, FILE_ALL_ACCESS, sid) == 0 {
            return Err(format!("AddAccessAllowedAceEx failed ({})", GetLastError()));
        }
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        let status = SetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl,
            std::ptr::null_mut(),
        );
        if status != ERROR_SUCCESS {
            return Err(format!(
                "SetNamedSecurityInfoW failed for {} ({status})",
                path.display()
            ));
        }
    }
    Ok(())
}
