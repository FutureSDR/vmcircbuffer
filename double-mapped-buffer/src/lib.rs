mod double_mapped_buffer;
pub use double_mapped_buffer::DoubleMappedBuffer;

#[cfg(windows)]
mod win;
#[cfg(windows)]
use win::DoubleMappedBufferImpl;
#[cfg(windows)]
use win::DoubleMappedBufferError;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
use unix::DoubleMappedBufferImpl;
#[cfg(not(windows))]
use unix::DoubleMappedBufferError;


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
