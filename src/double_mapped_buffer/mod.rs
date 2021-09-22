//! Underlying data structure that maps a buffer twice into virtual memory.

#[allow(clippy::module_inception)]
mod double_mapped_buffer;
pub use double_mapped_buffer::DoubleMappedBuffer;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::DoubleMappedBufferImpl;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix::DoubleMappedBufferImpl;

use thiserror::Error;
/// Errors that can occur when setting up the double mapping.
#[derive(Error, Debug)]
pub enum DoubleMappedBufferError {
    #[error("Failed to close temp file.")]
    Close,
    #[error("Failed to unmap second half.")]
    UnmapSecond,
    #[error("Failed to mmap second half.")]
    MapSecond,
    #[error("Failed to mmap first half.")]
    MapFirst,
    #[error("Failed to mmap placeholder.")]
    Placeholder,
    #[error("Failed to truncate temp file.")]
    Truncate,
    #[error("Failed to unlinkt temp file.")]
    Unlink,
    #[error("Failed to create temp file.")]
    Create,
    #[error("Wrong buffer alignment for data type.")]
    Alignment,
}

// =================== PAGESIZE ======================
use once_cell::sync::OnceCell;
static PAGE_SIZE: OnceCell<usize> = OnceCell::new();

/// Size of virtual memory pages.
///
/// Determines the granularity of the double buffer, which has to be a multiple
/// of the page size.
#[cfg(unix)]
pub fn pagesize() -> usize {
    *PAGE_SIZE.get_or_init(|| {
        unsafe {
            let ps = libc::sysconf(libc::_SC_PAGESIZE);
            if ps < 0 {
                panic!("could not determince page size");
            }
            ps as usize
        }
    })
}

#[cfg(windows)]
use winapi::um::sysinfoapi::GetSystemInfo;
#[cfg(windows)]
use winapi::um::sysinfoapi::SYSTEM_INFO;
#[cfg(windows)]
pub fn pagesize() -> usize {
    *PAGE_SIZE.get_or_init(|| {
        unsafe {
            let mut info: SYSTEM_INFO = std::mem::zeroed();
            GetSystemInfo(&mut info);
            info.dwAllocationGranularity as usize
        }
    })
}

