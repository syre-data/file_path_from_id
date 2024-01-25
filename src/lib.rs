#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::path_from_id;

#[cfg(target_os = "windows")]
pub use windows::Error;
