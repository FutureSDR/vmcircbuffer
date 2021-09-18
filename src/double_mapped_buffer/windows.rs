use winapi::shared::minwindef::DWORD;
use winapi::shared::minwindef::LPCVOID;
use winapi::shared::minwindef::LPVOID;
use winapi::um::handleapi::CloseHandle;
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::memoryapi::MapViewOfFileEx;
use winapi::um::memoryapi::VirtualAlloc;
use winapi::um::memoryapi::VirtualFree;
use winapi::um::winnt::HANDLE;
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

#[derive(Debug)]
pub struct DoubleMappedBufferImpl {
    addr: usize,
    handle: usize,
    size_bytes: usize,
    item_size: usize,
}

impl DoubleMappedBufferImpl {
    pub fn new(
        min_items: usize,
        item_size: usize,
        alignment: usize,
    ) -> Result<Self, DoubleMappedBufferError> {
        for _ in 0..5 {
            let ret = Self::new_try(min_items, item_size, alignment);
            if ret.is_ok() {
                return ret;
            }
        }
        Self::new_try(min_items, item_size, alignment)
    }

    fn new_try(
        min_items: usize,
        item_size: usize,
        alignment: usize,
    ) -> Result<Self, DoubleMappedBufferError> {
        let mut size = pagesize();
        while size < min_items * item_size || size % item_size != 0 {
            size += pagesize();
        }

        unsafe {
            let handle = CreateFileMappingA(
                INVALID_HANDLE_VALUE,
                std::mem::zeroed(),
                PAGE_READWRITE,
                0,
                size as DWORD,
                std::ptr::null(),
            );

            if handle == INVALID_HANDLE_VALUE || handle == 0 as LPVOID {
                return Err(DoubleMappedBufferError::Placeholder);
            }

            let first_tmp =
                VirtualAlloc(std::ptr::null_mut(), 2 * size, MEM_RESERVE, PAGE_NOACCESS);
            if first_tmp.is_null() {
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

            let first_ptr = (first_tmp as *mut u8).add(size) as LPVOID;
            let second_cpy = MapViewOfFileEx(handle, FILE_MAP_WRITE, 0, 0, size, first_ptr);
            if second_cpy != first_ptr {
                UnmapViewOfFile(first_cpy);
                CloseHandle(handle);
                return Err(DoubleMappedBufferError::MapSecond);
            }

            Ok(DoubleMappedBufferImpl {
                addr: first_tmp as usize,
                handle: handle as usize,
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
            UnmapViewOfFile(self.addr as LPCVOID);
            UnmapViewOfFile((self.addr + self.size_bytes) as LPCVOID);
            CloseHandle(self.handle as HANDLE);
        }
    }
}
