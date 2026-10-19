// Rust | startup.rs | Windows | explicit opt-in user login item
#[cfg(windows)]
mod platform {
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::*,
    };
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE: &str = "skillissue";
    pub type Snapshot = Option<(u32, Vec<u8>)>;

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    fn open(write: bool) -> Result<Option<Key>, String> {
        let mut key = std::ptr::null_mut();
        let path = wide(RUN_KEY);
        let status = unsafe {
            if write {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_QUERY_VALUE | KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut key,
                    std::ptr::null_mut(),
                )
            } else {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    KEY_QUERY_VALUE,
                    &mut key,
                )
            }
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(format!(
                "Open login item registry key: Windows error {status}."
            ));
        }
        Ok(Some(Key(key)))
    }
    pub fn snapshot() -> Result<Snapshot, String> {
        let Some(key) = open(false)? else {
            return Ok(None);
        };
        let name = wide(VALUE);
        let mut length = 0;
        let mut kind = 0;
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                std::ptr::null_mut(),
                &mut length,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(format!("Read login item: Windows error {status}."));
        }
        if length > 32_768 {
            return Err("The existing login item exceeds 32 KiB.".into());
        }
        let mut bytes = vec![0u8; length as usize];
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null(),
                &mut kind,
                bytes.as_mut_ptr(),
                &mut length,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!("Read login item: Windows error {status}."));
        }
        bytes.truncate(length as usize);
        Ok(Some((kind, bytes)))
    }
    pub fn restore(value: &Snapshot) -> Result<(), String> {
        let key = open(true)?.ok_or("Login item registry key is unavailable.")?;
        let name = wide(VALUE);
        let status = unsafe {
            if let Some((kind, bytes)) = value {
                RegSetValueExW(
                    key.0,
                    name.as_ptr(),
                    0,
                    *kind,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                )
            } else {
                RegDeleteValueW(key.0, name.as_ptr())
            }
        };
        if status == ERROR_SUCCESS || value.is_none() && status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("Update login item: Windows error {status}."))
        }
    }
    pub fn set(enabled: bool) -> Result<(), String> {
        if !enabled {
            return restore(&None);
        }
        let path =
            std::env::current_exe().map_err(|e| format!("Find application for login item: {e}"))?;
        let command = super::quoted_command(&path)?;
        let bytes = wide(&command)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        restore(&Some((REG_SZ, bytes)))
    }
}
#[cfg(windows)]
pub use platform::{restore, set, snapshot};

#[cfg(not(windows))]
pub fn snapshot() -> Result<Option<()>, String> {
    Ok(None)
}
#[cfg(not(windows))]
pub fn restore(_: &Option<()>) -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
pub fn set(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("Open at login is currently supported on Windows.".into())
    } else {
        Ok(())
    }
}

fn quoted_command(path: &std::path::Path) -> Result<String, String> {
    let path = path
        .to_str()
        .ok_or("The application path is not Unicode.")?;
    if path.contains(['"', '\r', '\n']) {
        return Err("The application path contains invalid command characters.".into());
    }
    Ok(format!("\"{path}\""))
}

#[cfg(test)]
mod tests {
    #[test]
    fn startup_command_quotes_the_exact_executable_without_launching_or_registry_writes() {
        assert_eq!(
            super::quoted_command(std::path::Path::new(
                r"C:\Program Files\skillissue\skillissue.exe"
            ))
            .unwrap(),
            r#""C:\Program Files\skillissue\skillissue.exe""#
        );
        assert!(super::quoted_command(std::path::Path::new("bad\"path.exe")).is_err());
    }
}
