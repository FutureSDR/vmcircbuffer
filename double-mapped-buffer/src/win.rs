use std::sync::atomic::{AtomicUsize, Ordering};
use winapi::shared::minwindef::LPVOID;
use winapi::um::handleapi::CloseHandle;
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::memoryapi::MapViewOfFileEx;
use winapi::um::memoryapi::VirtualAlloc;
use winapi::um::memoryapi::VirtualFree;
use winapi::um::winnt::MEM_RELEASE;
use winapi::um::winnt::MEM_RESERVE;
use winapi::um::winnt::PAGE_NOACCESS;
use winapi::um::winnt::PAGE_READWRITE;
use winapi::um::{
    memoryapi::{UnmapViewOfFile, FILE_MAP_WRITE},
    winbase::CreateFileMappingA,
};

use super::pagesize;
use super::DoubleMappedBufferError;

static SEGMENTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct DoubleMappedBufferImpl {
    addr: usize,
    handle: *mut libc::c_void,
    size_bytes: usize,
    item_size: usize,
}

impl DoubleMappedBufferImpl {
    pub fn new(
        min_items: usize,
        item_size: usize,
        alignment: usize,
    ) -> Result<Self, DoubleMappedBufferError> {
        Self::with_tmp_dir(min_items, item_size, alignment, "")
    }

    pub fn with_tmp_dir(
        min_items: usize,
        item_size: usize,
        alignment: usize,
        tmp_dir: &str,
    ) -> Result<Self, DoubleMappedBufferError> {
        for _ in 0..5 {
            let ret = Self::new_try(min_items, item_size, alignment, tmp_dir);
            if ret.is_ok() {
                return ret;
            }
        }
        Self::new_try(min_items, item_size, alignment, tmp_dir)
    }

    fn new_try(
        min_items: usize,
        item_size: usize,
        alignment: usize,
        tmp_dir: &str,
    ) -> Result<Self, DoubleMappedBufferError> {
        let mut size = pagesize();
        while size < min_items * item_size || size % item_size != 0 {
            size += pagesize();
        }

        let s = SEGMENTS.fetch_add(1, Ordering::SeqCst);
        let seg_name = format!("{}futuresdr-{}-{}", tmp_dir, std::process::id(), s);

        unsafe {
            let handle = CreateFileMappingA(
                INVALID_HANDLE_VALUE,
                std::mem::zeroed(),
                PAGE_READWRITE,
                0,
                size as u32,
                seg_name.as_ptr() as *const i8,
            );

            if handle == INVALID_HANDLE_VALUE || handle == 0 as LPVOID {
                return Err(DoubleMappedBufferError::Placeholder);
            }

            let first_tmp = VirtualAlloc(0 as LPVOID, 2 * size, MEM_RESERVE, PAGE_NOACCESS);
            if first_tmp == 0 as LPVOID {
                CloseHandle(handle);
                return Err(DoubleMappedBufferError::MapFirst);
            }

            let res = VirtualFree(first_tmp, 0, MEM_RELEASE);
            if res == 0 {
                CloseHandle(handle);
                return Err(DoubleMappedBufferError::MapSecond);
            }

            let first_cpy = MapViewOfFileEx(handle, FILE_MAP_WRITE, 0, 0, size, first_tmp);
            if first_tmp != first_cpy {
                CloseHandle(handle);
                return Err(DoubleMappedBufferError::MapFirst);
            }

            if first_tmp as usize % alignment != 0 {
                CloseHandle(handle);
                return Err(DoubleMappedBufferError::Alignment);
            }

            let second_cpy =
                MapViewOfFileEx(handle, FILE_MAP_WRITE, 0, 0, size, first_tmp.add(size));
            if second_cpy != first_tmp.add(size) {
                UnmapViewOfFile(first_cpy);
                CloseHandle(handle);
                return Err(DoubleMappedBufferError::MapSecond);
            }

            Ok(DoubleMappedBufferImpl {
                addr: first_tmp as usize,
                handle,
                size_bytes: size,
                item_size,
            })
        }
    }

    pub fn addr(&self) -> usize {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.size_bytes / self.item_size
    }
}

impl Drop for DoubleMappedBufferImpl {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(self.addr);
            UnmapViewOfFile(self.addr.add(self.size));
            CloseHandle(self.handle);
        }
    }
}
