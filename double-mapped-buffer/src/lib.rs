mod double_mapped_buffer;
pub use crate::double_mapped_buffer::DoubleMappedBuffer;

#[cfg(windows)]
mod win;
#[cfg(windows)]
use win::DoubleMappedBufferImpl;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix::DoubleMappedBufferImpl;

use thiserror::Error;
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
    #[error("Wrong buffer alignemnt for data type.")]
    Alignment,
}

// =================== PAGESIZE ======================
#[cfg(unix)]
pub fn pagesize() -> usize {
    unsafe {
        let ps = libc::sysconf(libc::_SC_PAGESIZE);
        if ps < 0 {
            panic!("could not determince page size");
        }
        ps as usize
    }
}

#[cfg(windows)]
use winapi::um::sysinfoapi::GetSystemInfo;
#[cfg(windows)]
use winapi::um::sysinfoapi::SYSTEM_INFO;

#[cfg(windows)]
pub fn pagesize() -> usize {
    unsafe {
        let mut info: SYSTEM_INFO = std::mem::zeroed();
        GetSystemInfo(&mut info);
        info.dwAllocationGranularity as usize
    }
}
