#[cfg(all(target_os = "linux", feature = "systemd"))]
pub mod systemd;

#[cfg(target_os = "linux")]
pub mod file;

#[path = "timezone_lookup.rs"]
pub mod lookup;
