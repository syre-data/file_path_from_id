#[cfg(target_family = "windows")]
pub mod windows;

#[cfg(target_family = "windows")]
pub use windows::path_from_id;

#[cfg(target_family = "windows")]
pub use windows::Error;

#[cfg(target_family = "unix")]
pub mod unix;

#[cfg(target_family = "unix")]
pub use unix::path_from_id;

#[cfg(target_family = "unix")]
pub use unix::Error;
