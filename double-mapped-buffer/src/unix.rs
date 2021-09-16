use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use thiserror::Error;

use super::pagesize;

#[derive(Error, Debug)]
pub enum DoubleMappedBufferError {
    #[error("Failed to close temp file.")]
    Close,
    #[error("Failed to unmap second half.")]
    UnmapSecond,
    #[error("Failed to mmap second half.")]
    MapSecond,
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

#[derive(Debug)]
pub struct DoubleMappedBufferImpl {
    addr: *mut libc::c_void,
    size_bytes: usize,
    item_size: usize,
}

impl DoubleMappedBufferImpl {
    pub fn new(min_items: usize, item_size: usize, alignment: usize) -> Result<Self, DoubleMappedBufferError> {
        Self::with_tmp_dir(min_items, item_size, alignment, "/tmp/")
    }

    pub fn with_tmp_dir(min_items: usize, item_size: usize, alignment: usize, tmp_dir: &str) -> Result<Self, DoubleMappedBufferError> {
        for _ in 0..5 {
            let ret = Self::new_try(min_items, item_size, alignment, tmp_dir);
            if ret.is_ok() {
                return ret;
            }
        }
        Self::new_try(min_items, item_size, alignment, tmp_dir)
    }

    fn new_try(min_items: usize, item_size: usize, alignment: usize, tmp_dir: &str) -> Result<Self, DoubleMappedBufferError> {

        let mut size = pagesize();
        while size < min_items * item_size || size % item_size != 0 {
            size += pagesize();
        }

        let tmp = tmp_dir.to_string();
        let mut path = PathBuf::new();
        path.push(tmp);
        path.push("buffer-XXXXXX");
        let cstring = CString::new(path.into_os_string().as_bytes()).unwrap();
        let path = cstring.as_bytes_with_nul().as_ptr();

        let fd;
        let buff;
        unsafe {
            fd = libc::mkstemp(path as *mut libc::c_char);
            if fd < 0 {
                return Err(DoubleMappedBufferError::Create);
            }

            let ret = libc::unlink(path.cast::<i8>());
            if ret < 0 {
                libc::close(fd);
                return Err(DoubleMappedBufferError::Unlink);
            }

            let ret = libc::ftruncate(fd, 2 * size as libc::off_t);
            if ret < 0 {
                libc::close(fd);
                return Err(DoubleMappedBufferError::Truncate);
            }

            buff = libc::mmap(
                std::ptr::null_mut::<libc::c_void>(),
                2 * size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            if buff == libc::MAP_FAILED {
                libc::close(fd);
                return Err(DoubleMappedBufferError::Placeholder);
            }
            if buff as usize % alignment != 0 {
                libc::close(fd);
                return Err(DoubleMappedBufferError::Alignment);
            }

            let ret = libc::munmap(buff.add(size), size);
            if ret < 0 {
                libc::munmap(buff, size);
                libc::close(fd);
                return Err(DoubleMappedBufferError::UnmapSecond);
            }

            let buff2 = libc::mmap(
                buff.add(size),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            if buff2 != buff.add(size) {
                libc::munmap(buff, size);
                libc::close(fd);
                return Err(DoubleMappedBufferError::MapSecond);
            }

            let ret = libc::ftruncate(fd, size as libc::off_t);
            if ret < 0 {
                libc::munmap(buff, size);
                libc::munmap(buff2, size);
                libc::close(fd);
                return Err(DoubleMappedBufferError::Truncate);
            }

            let ret = libc::close(fd);
            if ret < 0 {
                return Err(DoubleMappedBufferError::Close);
            }
        }

        Ok(DoubleMappedBufferImpl { addr: buff, size_bytes: size, item_size })
    }

    pub fn addr(&self) -> *mut libc::c_void {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.size_bytes / self.item_size
    }
}

impl Drop for DoubleMappedBufferImpl {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.addr, self.size_bytes * 2);
        }
    }
}

