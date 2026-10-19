// Rust | lib.rs | native Windows desktop integration
pub mod shortcuts;
pub mod tray;
pub mod window;

/// Reveal a generated local artifact in the native file manager.
pub fn reveal_path(path: &std::path::Path) -> Result<(), String> {
    validate_reveal_path(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            System::Com::{
                COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{SHOpenFolderAndSelectItems, SHParseDisplayName},
        };
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err("The export path contains a null character.".into());
        }
        wide.push(0);
        // This synchronous adapter runs on a blocking worker. Balance COM and
        // shell allocations on that same thread, including failed selections.
        unsafe {
            let initialized = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            if initialized < 0 {
                return Err(format!(
                    "Initialize export location: HRESULT {initialized:#x}"
                ));
            }
            let mut item = std::ptr::null_mut();
            let parsed = SHParseDisplayName(
                wide.as_ptr(),
                std::ptr::null_mut(),
                &mut item,
                0,
                std::ptr::null_mut(),
            );
            let result = if parsed < 0 {
                Err(format!("Resolve export location: HRESULT {parsed:#x}"))
            } else {
                let selected = SHOpenFolderAndSelectItems(item, 0, std::ptr::null(), 0);
                if selected < 0 {
                    Err(format!("Reveal export in Explorer: HRESULT {selected:#x}"))
                } else {
                    Ok(())
                }
            };
            if !item.is_null() {
                CoTaskMemFree(item.cast());
            }
            CoUninitialize();
            result
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Revealing exports is only supported on Windows.".into())
    }
}

fn validate_reveal_path(path: &std::path::Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Cannot reveal a relative export path.".into());
    }
    match path.metadata() {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err("The export path is not a file.".into()),
        Err(error) => Err(format!("The exported file is unavailable: {error}")),
    }
}

pub fn initialize_app_identity() -> Result<(), String> {
    #[cfg(windows)]
    {
        let id: Vec<u16> = skillissue_domain::identity::APPLICATION_ID
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // Explicit identity prevents inheriting a launching application's taskbar group.
        let status = unsafe {
            windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr())
        };
        if status < 0 {
            return Err(format!(
                "Set skillissue application identity: HRESULT {status:#x}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod export_tests {
    use super::validate_reveal_path;

    #[test]
    #[ignore = "opens Explorer; requires an explicit disposable export path and interactive desktop"]
    fn native_reveal_selects_acceptance_file() {
        let path = std::env::var_os("SKILLISSUE_REVEAL_TEST_FILE")
            .map(std::path::PathBuf::from)
            .expect("set SKILLISSUE_REVEAL_TEST_FILE to a disposable existing file");
        super::reveal_path(&path).expect("Windows accepted the reveal request");
        println!("Verify Explorer selected: {}", path.display());
    }

    #[test]
    fn reveal_requires_an_existing_absolute_file_without_launching_explorer() {
        assert!(validate_reveal_path(std::path::Path::new("relative.md")).is_err());
        let directory = std::env::temp_dir();
        assert!(validate_reveal_path(&directory).is_err());
        let path = directory.join(format!(
            "skillissue-reveal-{}-{}.md",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(validate_reveal_path(&path).is_err());
        std::fs::write(&path, "export fixture").unwrap();
        let result = validate_reveal_path(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(result.is_ok());
        assert!(validate_reveal_path(&path).is_err());
    }
}
