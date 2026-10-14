//! The macOS sheets (`sheets.rs`) on Linux.

use crate::NativeWindow;

/// The macOS Full Disk Access reminder. Linux has no such permission, so
/// there is nothing to remind about.
///
/// # Safety
///
/// Always safe on Linux; `unsafe` matches the macOS signature.
pub unsafe fn remind_full_disk_access(_window: NativeWindow) {}
