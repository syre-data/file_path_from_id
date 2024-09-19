#[cfg(target_family = "windows")]
pub mod windows;

#[cfg(target_family = "windows")]
pub use windows::{path_from_file, path_from_id, Error};

#[cfg(target_family = "unix")]
pub mod unix;

#[cfg(target_family = "unix")]
pub use unix::{path_from_id, Error};
