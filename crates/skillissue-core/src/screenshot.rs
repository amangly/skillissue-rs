// Rust | screenshot.rs | shared native runtime | Windows 10/11 GDI.
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

pub fn capture(monitor_point: Option<(i32, i32)>) -> Result<Screenshot, String> {
    #[cfg(windows)]
    {
        windows_capture::capture(monitor_point)
    }
    #[cfg(not(windows))]
    {
        let _ = monitor_point;
        Err("Native screenshot capture currently requires Windows.".into())
    }
}

#[cfg(windows)]
mod windows_capture {
    use super::Screenshot;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use std::io::Cursor;
    use std::mem::size_of;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::*;

    struct DesktopDc(HDC);

    impl Drop for DesktopDc {
        fn drop(&mut self) {
            unsafe { ReleaseDC(null_mut(), self.0) };
        }
    }

    struct MemoryDc(HDC);

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            unsafe { DeleteDC(self.0) };
        }
    }

    struct Bitmap(HBITMAP);

    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe { DeleteObject(self.0) };
        }
    }

    struct SelectedBitmap {
        dc: HDC,
        previous: HGDIOBJ,
    }

    impl Drop for SelectedBitmap {
        fn drop(&mut self) {
            unsafe { SelectObject(self.dc, self.previous) };
        }
    }

    fn win_error(operation: &str) -> String {
        format!("{operation}: {}", std::io::Error::last_os_error())
    }

    pub fn capture(monitor_point: Option<(i32, i32)>) -> Result<Screenshot, String> {
        let (point, flags) = match monitor_point {
            Some((x, y)) => (POINT { x, y }, MONITOR_DEFAULTTONEAREST),
            None => (POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
        };
        let monitor = unsafe { MonitorFromPoint(point, flags) };
        if monitor.is_null() {
            return Err(win_error("Find monitor"));
        }
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err(win_error("Read monitor geometry"));
        }
        let rect = info.rcMonitor;
        let width = rect
            .right
            .checked_sub(rect.left)
            .ok_or("Invalid monitor width")?;
        let height = rect
            .bottom
            .checked_sub(rect.top)
            .ok_or("Invalid monitor height")?;
        if width <= 0 || height <= 0 || width as u64 * height as u64 > 64_000_000 {
            return Err(
                "Monitor dimensions are invalid or exceed the 64 megapixel capture limit.".into(),
            );
        }
        let desktop = DesktopDc(unsafe { GetDC(null_mut()) });
        if desktop.0.is_null() {
            return Err(win_error("Acquire desktop DC"));
        }
        let memory = MemoryDc(unsafe { CreateCompatibleDC(desktop.0) });
        if memory.0.is_null() {
            return Err(win_error("Create screenshot DC"));
        }
        let bitmap = Bitmap(unsafe { CreateCompatibleBitmap(desktop.0, width, height) });
        if bitmap.0.is_null() {
            return Err(win_error("Create screenshot bitmap"));
        }
        {
            let previous = unsafe { SelectObject(memory.0, bitmap.0) };
            if previous.is_null() || previous as isize == -1 {
                return Err(win_error("Select screenshot bitmap"));
            }
            let _selected = SelectedBitmap {
                dc: memory.0,
                previous,
            };
            if unsafe {
                BitBlt(
                    memory.0,
                    0,
                    0,
                    width,
                    height,
                    desktop.0,
                    rect.left,
                    rect.top,
                    SRCCOPY | CAPTUREBLT,
                )
            } == 0
            {
                return Err(win_error("Copy monitor pixels"));
            }
        }
        // GetDIBits requires the bitmap to be deselected from the memory DC.
        let mut dib = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        let lines = unsafe {
            GetDIBits(
                desktop.0,
                bitmap.0,
                0,
                height as u32,
                pixels.as_mut_ptr().cast(),
                &mut dib,
                DIB_RGB_COLORS,
            )
        };
        if lines != height {
            return Err(win_error("Read screenshot pixels"));
        }
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let image = RgbaImage::from_raw(width as u32, height as u32, pixels)
            .ok_or("Screenshot pixel dimensions are inconsistent.")?;
        let width = image.width();
        let height = image.height();
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut png, ImageFormat::Png)
            .map_err(|error| format!("Encode screenshot PNG: {error}"))?;
        Ok(Screenshot {
            data_url: format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png.into_inner())
            ),
            width,
            height,
        })
    }
}

/// Re-encode local PNG capture within the hosted upload bound; BYOK keeps the original.
pub fn fit_hosted(mut screenshot: Screenshot) -> Result<Screenshot, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use image::ImageFormat;
    use std::io::Cursor;
    const LIMIT: usize = 4 * 1024 * 1024;
    let encoded = screenshot
        .data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or("Screen capture must contain a PNG image.")?;
    if encoded.len() > 32 * 1024 * 1024 {
        return Err("Screen capture exceeds the local processing limit.".into());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Screen capture contains invalid image data.")?;
    let dimensions = image::ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png)
        .into_dimensions()
        .map_err(|_| "Screen capture contains invalid PNG data.")?;
    if dimensions.0 == 0 || dimensions.1 == 0 || dimensions.0 > 4096 || dimensions.1 > 4096 {
        return Err(
            "Full-resolution screen capture exceeds Palepale's 4096-pixel per-side limit.".into(),
        );
    }
    if bytes.len() <= LIMIT {
        return Ok(screenshot);
    }
    let rgb = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
        .map_err(|_| "Screen capture contains invalid PNG data.")?
        .into_rgb8();
    for quality in [95, 90, 85] {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode_image(&rgb)
            .map_err(|_| "Could not compress the hosted screen capture.")?;
        if jpeg.len() <= LIMIT {
            screenshot.width = rgb.width();
            screenshot.height = rgb.height();
            screenshot.data_url = format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg));
            return Ok(screenshot);
        }
    }
    Err(
        "Full-resolution screen capture exceeds the 4 MiB upload limit at readable JPEG quality."
            .into(),
    )
}

#[cfg(test)]
mod hosted_tests {
    #[test]
    fn noisy_screen_is_compressed_without_shrinking_pixel_dimensions() {
        use super::*;
        use base64::{Engine, engine::general_purpose::STANDARD};
        use std::io::Cursor;
        let mut image = image::RgbaImage::new(1920, 1080);
        let mut state = 0x11223344_u32;
        for pixel in image.pixels_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *pixel = image::Rgba([state as u8, (state >> 8) as u8, (state >> 16) as u8, 255]);
        }
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert!(png.get_ref().len() > 4 * 1024 * 1024);
        let fitted = fit_hosted(Screenshot {
            data_url: format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png.into_inner())
            ),
            width: 1920,
            height: 1080,
        })
        .unwrap();
        assert_eq!((fitted.width, fitted.height), (1920, 1080));
        assert!(fitted.data_url.starts_with("data:image/jpeg;base64,"));
        let bytes = STANDARD
            .decode(fitted.data_url.split_once(',').unwrap().1)
            .unwrap();
        assert!(bytes.len() <= 4 * 1024 * 1024);
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (fitted.width, fitted.height)
        );
    }
}
