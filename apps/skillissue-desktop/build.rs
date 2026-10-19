//! Windows executable metadata and native UI thread stack reserve.
#[cfg(windows)]
#[path = "build/windows.rs"]
mod windows;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=skillissue=/STACK:8388608");
        #[cfg(windows)]
        windows::embed_resources().expect("compile skillissue Windows resources");
    }
}
