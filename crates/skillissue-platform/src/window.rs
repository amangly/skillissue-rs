// Rust | window.rs | Windows HWND controls
use crate::shortcuts::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    Glass,
    Opaque,
}

pub use skillissue_domain::shortcuts::clamp_movement;

/// A borrowed handle. The framework continues to own and destroy the window.
#[derive(Debug, Clone, Copy)]
pub struct NativeWindow {
    handle: isize,
}

/// A temporarily hidden, non-minimized window. Restoration changes visibility only,
/// preserving the existing bounds, maximize state, and foreground focus.
#[derive(Debug)]
pub struct HiddenWindow {
    window: NativeWindow,
    restored: bool,
}

impl HiddenWindow {
    pub fn restore(&mut self) -> Result<(), String> {
        if !self.restored {
            self.window.restore_after_capture()?;
            self.restored = true;
        }
        Ok(())
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(windows)]
mod implementation {
    use super::*;
    use std::{ffi::c_void, mem::size_of};
    use windows_sys::Win32::{
        Foundation::{GetLastError, HWND, RECT, SetLastError},
        Graphics::{
            Dwm::{
                DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
                DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
                DwmSetWindowAttribute,
            },
            Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
        },
        UI::{
            Controls::MARGINS,
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, HWND_NOTOPMOST, HWND_TOPMOST,
                IsIconic, IsWindow, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW,
                SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE,
                SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow,
                SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WS_EX_NOACTIVATE,
            },
        },
    };

    fn last_error(operation: &str) -> String {
        format!("{operation}: {}", std::io::Error::last_os_error())
    }

    impl NativeWindow {
        pub fn configure_tray_window(self) -> Result<(), String> {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GWL_STYLE, GWLP_HINSTANCE, GetWindowLongPtrW, ICON_BIG, ICON_SMALL, IMAGE_ICON,
                LR_SHARED, LoadImageW, SendMessageW, WM_SETICON, WS_CAPTION, WS_EX_APPWINDOW,
                WS_EX_TOOLWINDOW, WS_SYSMENU,
            };
            let hwnd = self.hwnd()?;
            unsafe {
                SetLastError(0);
                let old = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if old == 0 && GetLastError() != 0 {
                    return Err(last_error("Read window style"));
                }
                let new = (old & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize;
                SetLastError(0);
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                if style == 0 && GetLastError() != 0 {
                    return Err(last_error("Read window frame style"));
                }
                let frameless = style & !((WS_CAPTION | WS_SYSMENU) as isize);
                if style != frameless {
                    SetLastError(0);
                    if SetWindowLongPtrW(hwnd, GWL_STYLE, frameless) == 0 && GetLastError() != 0 {
                        return Err(last_error("Remove native close panel"));
                    }
                }
                if old != new || style != frameless {
                    SetLastError(0);
                    if SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new) == 0 && GetLastError() != 0 {
                        return Err(last_error("Set tray window style"));
                    }
                    if SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    ) == 0
                    {
                        return Err(last_error("Refresh tray window style"));
                    }
                }
                let instance = GetWindowLongPtrW(hwnd, GWLP_HINSTANCE)
                    as windows_sys::Win32::Foundation::HINSTANCE;
                for (kind, size) in [(ICON_SMALL, 16), (ICON_BIG, 32)] {
                    let icon = LoadImageW(
                        instance,
                        std::ptr::without_provenance(1), // MAKEINTRESOURCEW(1), not a dereferenceable pointer.
                        IMAGE_ICON,
                        size,
                        size,
                        LR_SHARED,
                    );
                    if icon.is_null() {
                        return Err(last_error("Load application logo"));
                    }
                    SendMessageW(hwnd, WM_SETICON, kind as usize, icon as isize);
                }
            }
            Ok(())
        }
        pub fn start_drag(self) -> Result<(), String> {
            use windows_sys::Win32::UI::{
                Input::KeyboardAndMouse::ReleaseCapture,
                WindowsAndMessaging::{HTCAPTION, PostMessageW, SC_MOVE, WM_SYSCOMMAND},
            };
            let hwnd = self.hwnd()?;
            // Queue the OS move loop after GPUI releases its event-handler borrow.
            unsafe { ReleaseCapture() };
            if unsafe { PostMessageW(hwnd, WM_SYSCOMMAND, (SC_MOVE | HTCAPTION) as usize, 0) } == 0
            {
                return Err(last_error("Start window drag"));
            }
            Ok(())
        }

        pub fn from_handle(handle: isize) -> Result<Self, String> {
            let result = Self { handle };
            result.hwnd()?;
            Ok(result)
        }

        fn hwnd(self) -> Result<HWND, String> {
            let hwnd = self.handle as HWND;
            if hwnd.is_null() || unsafe { IsWindow(hwnd) } == 0 {
                return Err("Native window is no longer available.".into());
            }
            Ok(hwnd)
        }

        pub fn handle(self) -> isize {
            self.handle
        }

        /// Requests the documented OS capture exclusion; capture tools may behave differently.
        pub fn set_capture_protection(self, enabled: bool) -> Result<(), String> {
            let hwnd = self.hwnd()?;
            let affinity = if enabled {
                WDA_EXCLUDEFROMCAPTURE
            } else {
                WDA_NONE
            };
            if unsafe { SetWindowDisplayAffinity(hwnd, affinity) } == 0 {
                return Err(last_error("Set capture protection"));
            }
            Ok(())
        }

        pub fn set_no_focus(self, enabled: bool) -> Result<(), String> {
            let hwnd = self.hwnd()?;
            unsafe { SetLastError(0) };
            let old = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
            if old == 0 && unsafe { GetLastError() } != 0 {
                return Err(last_error("Read native focus mode"));
            }
            let mask = WS_EX_NOACTIVATE as isize;
            let new = if enabled { old | mask } else { old & !mask };
            if old == new {
                return Ok(());
            }
            unsafe { SetLastError(0) };
            if unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new) } == 0
                && unsafe { GetLastError() } != 0
            {
                return Err(last_error("Set native focus mode"));
            }
            if unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )
            } == 0
            {
                return Err(last_error("Refresh native focus mode"));
            }
            Ok(())
        }

        pub fn set_always_on_top(self, enabled: bool) -> Result<(), String> {
            let hwnd = self.hwnd()?;
            if unsafe {
                SetWindowPos(
                    hwnd,
                    if enabled {
                        HWND_TOPMOST
                    } else {
                        HWND_NOTOPMOST
                    },
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            } == 0
            {
                return Err(last_error("Set always-on-top mode"));
            }
            Ok(())
        }

        /// Returns the material actually accepted by DWM. Unsupported glass falls back to opaque.
        /// The renderer must paint its opaque fallback when `Opaque` is returned.
        pub fn set_material(self, requested: Material) -> Result<Material, String> {
            let hwnd = self.hwnd()?;
            let backdrop: i32 = if requested == Material::Glass {
                DWMSBT_TRANSIENTWINDOW
            } else {
                DWMSBT_NONE
            };
            let status = unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_SYSTEMBACKDROP_TYPE as u32,
                    &backdrop as *const _ as *const c_void,
                    size_of::<i32>() as u32,
                )
            };
            let actual = if status >= 0 {
                requested
            } else {
                Material::Opaque
            };
            let corner: i32 = DWMWCP_ROUND;
            unsafe {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                    &corner as *const _ as *const c_void,
                    size_of::<i32>() as u32,
                );
            }
            let extent = if actual == Material::Glass { -1 } else { 0 };
            let margins = MARGINS {
                cxLeftWidth: extent,
                cxRightWidth: extent,
                cyTopHeight: extent,
                cyBottomHeight: extent,
            };
            let extended = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };
            if extended < 0 && actual == Material::Glass {
                let none: i32 = DWMSBT_NONE;
                unsafe {
                    let _ = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        &none as *const _ as *const c_void,
                        size_of::<i32>() as u32,
                    );
                }
                return Ok(Material::Opaque);
            }
            Ok(actual)
        }

        pub fn is_visible(self) -> Result<bool, String> {
            let hwnd = self.hwnd()?;
            Ok(unsafe { IsWindowVisible(hwnd) } != 0)
        }

        pub fn show(self, activate: bool) -> Result<(), String> {
            let hwnd = self.hwnd()?;
            unsafe {
                let command = if !activate {
                    SW_SHOWNOACTIVATE
                } else if IsIconic(hwnd) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOW
                };
                ShowWindow(hwnd, command);
                if activate {
                    // Windows may deny foreground activation according to its foreground-lock policy.
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            Ok(())
        }

        pub fn hide(self) -> Result<(), String> {
            let hwnd = self.hwnd()?;
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            Ok(())
        }

        pub fn hide_for_capture(self) -> Result<Option<HiddenWindow>, String> {
            let hwnd = self.hwnd()?;
            if unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { IsIconic(hwnd) } != 0 {
                return Ok(None);
            }
            if unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_HIDEWINDOW | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                )
            } == 0
            {
                return Err(last_error("Temporarily hide window for screen capture"));
            }
            Ok(Some(HiddenWindow {
                window: self,
                restored: false,
            }))
        }

        pub(super) fn restore_after_capture(self) -> Result<(), String> {
            let hwnd = self.hwnd()?;
            if unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                )
            } == 0
            {
                return Err(last_error("Restore window after screen capture"));
            }
            Ok(())
        }

        fn rect(self) -> Result<RECT, String> {
            let hwnd = self.hwnd()?;
            let mut rect = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
                return Err(last_error("Read window bounds"));
            }
            Ok(rect)
        }

        pub fn center(self) -> Result<(i32, i32), String> {
            let rect = self.rect()?;
            Ok((
                ((i64::from(rect.left) + i64::from(rect.right)) / 2) as i32,
                ((i64::from(rect.top) + i64::from(rect.bottom)) / 2) as i32,
            ))
        }

        pub fn move_by(self, dx: i32, dy: i32) -> Result<(), String> {
            let hwnd = self.hwnd()?;
            let rect = self.rect()?;
            let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
                return Err(last_error("Read current monitor work area"));
            }
            let area = info.rcWork;
            let x = clamp_movement(
                rect.left,
                dx,
                area.left,
                area.right.saturating_sub(area.left) as u32,
                rect.right.saturating_sub(rect.left) as u32,
            );
            let y = clamp_movement(
                rect.top,
                dy,
                area.top,
                area.bottom.saturating_sub(area.top) as u32,
                rect.bottom.saturating_sub(rect.top) as u32,
            );
            if unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            } == 0
            {
                return Err(last_error("Move native overlay"));
            }
            Ok(())
        }

        pub fn move_action(self, action: Action) -> Result<(), String> {
            let dpi = unsafe { GetDpiForWindow(self.hwnd()?) }.max(96);
            let step = (80.0 * f64::from(dpi) / 96.0).round() as i32;
            let (dx, dy) = match action {
                Action::MoveUp => (0, -step),
                Action::MoveDown => (0, step),
                Action::MoveLeft => (-step, 0),
                Action::MoveRight => (step, 0),
                _ => return Err("The requested action does not move the overlay.".into()),
            };
            self.move_by(dx, dy)
        }
    }
}

#[cfg(not(windows))]
impl NativeWindow {
    pub fn start_drag(self) -> Result<(), String> {
        Err("Native window dragging requires Windows.".into())
    }
    pub fn from_handle(_handle: isize) -> Result<Self, String> {
        Err("Native window controls currently support Windows only.".into())
    }
    pub fn handle(self) -> isize {
        self.handle
    }
    pub fn is_visible(self) -> Result<bool, String> {
        Err("Windows only.".into())
    }
    pub fn center(self) -> Result<(i32, i32), String> {
        Err("Windows only.".into())
    }
    pub fn set_capture_protection(self, _enabled: bool) -> Result<(), String> {
        Err("Windows only.".into())
    }
    pub fn configure_tray_window(self) -> Result<(), String> {
        Ok(())
    }
    pub fn set_no_focus(self, _enabled: bool) -> Result<(), String> {
        Err("Windows only.".into())
    }
    pub fn set_always_on_top(self, _enabled: bool) -> Result<(), String> {
        Err("Windows only.".into())
    }
    pub fn set_material(self, _requested: Material) -> Result<Material, String> {
        Err("Windows only.".into())
    }
    pub fn show(self, _activate: bool) -> Result<(), String> {
        Err("Windows only.".into())
    }
    pub fn hide(self) -> Result<(), String> {
        Err("Windows only.".into())
    }
    pub fn hide_for_capture(self) -> Result<Option<HiddenWindow>, String> {
        Err("Windows only.".into())
    }
    fn restore_after_capture(self) -> Result<(), String> {
        Err("Windows only.".into())
    }
    pub fn move_by(self, _dx: i32, _dy: i32) -> Result<(), String> {
        Err("Windows only.".into())
    }
    pub fn move_action(self, _action: Action) -> Result<(), String> {
        Err("Windows only.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_monitor_origins_and_oversized_windows_remain_reachable() {
        assert_eq!(clamp_movement(-1800, -80, -1920, 1920, 560), -1880);
        assert_eq!(clamp_movement(-1880, -80, -1920, 1920, 560), -1920);
        assert_eq!(clamp_movement(1300, 80, 0, 1920, 560), 1360);
        assert_eq!(clamp_movement(0, 80, 0, 300, 560), 0);
    }

    #[test]
    fn extreme_positions_do_not_overflow_before_clamping() {
        assert_eq!(clamp_movement(i32::MAX, i32::MAX, 0, 1920, 560), 1360);
        assert_eq!(clamp_movement(i32::MIN, i32::MIN, -1920, 1920, 560), -1920);
    }

    #[cfg(windows)]
    #[test]
    fn invalid_window_handle_is_a_recoverable_error() {
        assert!(NativeWindow::from_handle(0).is_err());
        assert!(NativeWindow::from_handle(-1).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn showing_a_minimized_window_restores_it_and_preserves_a_maximized_window() {
        use windows_sys::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GetForegroundWindow, IsIconic, IsZoomed,
                SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE, SetForegroundWindow, ShowWindow,
                WS_EX_NOACTIVATE, WS_OVERLAPPEDWINDOW,
            },
        };

        struct OwnedWindow(HWND, HWND);
        impl Drop for OwnedWindow {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.0);
                    if !self.1.is_null() {
                        SetForegroundWindow(self.1);
                    }
                }
            }
        }

        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "skillissue platform reveal test\0".encode_utf16().collect();
        let foreground = unsafe { GetForegroundWindow() };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                -32000,
                -32000,
                180,
                120,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null(), "{}", std::io::Error::last_os_error());
        let _owned = OwnedWindow(hwnd, foreground);
        let window = NativeWindow::from_handle(hwnd as isize).unwrap();

        unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE) };
        assert_ne!(unsafe { IsIconic(hwnd) }, 0);
        window.show(false).unwrap();
        assert_eq!(unsafe { IsIconic(hwnd) }, 0);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);

        unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE) };
        assert_ne!(unsafe { IsIconic(hwnd) }, 0);
        window.show(true).unwrap();
        assert_eq!(unsafe { IsIconic(hwnd) }, 0);

        unsafe { ShowWindow(hwnd, SW_SHOWMAXIMIZED) };
        assert_ne!(unsafe { IsZoomed(hwnd) }, 0);
        window.show(true).unwrap();
        assert_ne!(unsafe { IsZoomed(hwnd) }, 0);
    }

    #[cfg(windows)]
    #[test]
    fn drag_queues_native_move_command_without_entering_a_nested_loop() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HTCAPTION, MSG, PM_REMOVE, PeekMessageW, SC_MOVE,
            WM_SYSCOMMAND, WS_OVERLAPPEDWINDOW,
        };
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                -32000,
                -32000,
                180,
                120,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        let result = NativeWindow::from_handle(hwnd as isize)
            .unwrap()
            .start_drag();
        let mut message = MSG::default();
        let queued =
            unsafe { PeekMessageW(&mut message, hwnd, WM_SYSCOMMAND, WM_SYSCOMMAND, PM_REMOVE) };
        unsafe { DestroyWindow(hwnd) };
        result.unwrap();
        assert_ne!(queued, 0);
        assert_eq!(message.wParam, (SC_MOVE | HTCAPTION) as usize);
    }

    #[cfg(windows)]
    #[test]
    fn capture_guard_preserves_geometry_focus_and_minimized_visibility() {
        use std::mem::size_of;
        use windows_sys::Win32::{
            Foundation::{HWND, RECT},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GetForegroundWindow, GetWindowPlacement,
                GetWindowRect, IsIconic, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, ShowWindow,
                WINDOWPLACEMENT, WS_OVERLAPPEDWINDOW,
            },
        };

        struct OwnedWindow(HWND);
        impl Drop for OwnedWindow {
            fn drop(&mut self) {
                unsafe { DestroyWindow(self.0) };
            }
        }

        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "skillissue platform capture test\0"
            .encode_utf16()
            .collect();
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                -32000,
                -32000,
                180,
                120,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null(), "{}", std::io::Error::last_os_error());
        let _owned = OwnedWindow(hwnd);
        let window = NativeWindow::from_handle(hwnd as isize).unwrap();
        assert!(window.hide_for_capture().unwrap().is_none());
        let foreground = unsafe { GetForegroundWindow() };
        unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        let mut before = RECT::default();
        assert_ne!(unsafe { GetWindowRect(hwnd, &mut before) }, 0);
        let mut placement_before = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        assert_ne!(
            unsafe { GetWindowPlacement(hwnd, &mut placement_before) },
            0
        );
        {
            let _hidden = window.hide_for_capture().unwrap().unwrap();
            assert!(!window.is_visible().unwrap());
            assert!(window.hide_for_capture().unwrap().is_none());
        }
        assert!(window.is_visible().unwrap());
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        let mut after = RECT::default();
        assert_ne!(unsafe { GetWindowRect(hwnd, &mut after) }, 0);
        assert_eq!(
            (after.left, after.top, after.right, after.bottom),
            (before.left, before.top, before.right, before.bottom)
        );
        let mut placement_after = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        assert_ne!(unsafe { GetWindowPlacement(hwnd, &mut placement_after) }, 0);
        assert_eq!(placement_after.showCmd, placement_before.showCmd);
        unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE) };
        assert_ne!(unsafe { IsIconic(hwnd) }, 0);
        assert!(window.hide_for_capture().unwrap().is_none());
        assert_ne!(unsafe { IsIconic(hwnd) }, 0);
    }
}
