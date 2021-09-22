use std::marker::PhantomData;
use std::mem;
use std::slice;

use super::DoubleMappedBufferError;
use super::DoubleMappedBufferImpl;

/// A buffer that is mapped twice, back-to-back in the virtual address space of the process.
///
/// This struct is supposed to be used as a base for buffer implementations that
/// want to exploit the consequtive mappings to present available buffer space
/// sequentially, without having to worry about wrapping.
pub struct DoubleMappedBuffer<T> {
    buffer: DoubleMappedBufferImpl,
    _p: PhantomData<T>,
}

impl<T> DoubleMappedBuffer<T> {
    /// Create a buffer that can hold at least `min_items` items.
    ///
    /// The acutal capacity of the buffer will be the smallest multiple of the
    /// system page size and the item size that can hold at least `min_items`
    /// items.
    pub fn new(min_items: usize) -> Result<Self, DoubleMappedBufferError> {
        match DoubleMappedBufferImpl::new(min_items, mem::size_of::<T>(), mem::align_of::<T>()) {
            Ok(buffer) => Ok(DoubleMappedBuffer {
                buffer,
                _p: PhantomData,
            }),
            Err(e) => Err(e),
        }
    }

    /// Returns the slice corresponding to the first mapping of the buffer.
    ///
    /// # Safety
    ///
    /// Provides raw access to the slice.
    pub unsafe fn slice(&self) -> &[T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        slice::from_raw_parts(addr as *const T, self.buffer.capacity())
    }

    /// Returns the mutable slice corresponding to the first mapping of the buffer.
    ///
    /// # Safety
    ///
    /// Provides raw access to the slice.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn slice_mut(&self) -> &mut [T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        slice::from_raw_parts_mut(addr as *mut T, self.buffer.capacity())
    }

    /// View of the full buffer, shifted by an offset.
    ///
    /// # Safety
    ///
    /// Provides raw access to the slice. The offset has to be <= the
    /// [capacity](DoubleMappedBuffer::capacity) of the buffer.
    pub unsafe fn slice_with_offset(&self, offset: usize) -> &[T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        debug_assert!(offset <= self.buffer.capacity());
        slice::from_raw_parts((addr as *const T).add(offset), self.buffer.capacity())
    }

    /// Mutable view of the full buffer, shifted by an offset.
    ///
    /// # Safety
    ///
    /// Provides raw access to the slice. The offset has to be <= the
    /// [capacity](DoubleMappedBuffer::capacity) of the buffer.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn slice_with_offset_mut(&self, offset: usize) -> &mut [T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        debug_assert!(offset <= self.buffer.capacity());
        slice::from_raw_parts_mut((addr as *mut T).add(offset), self.buffer.capacity())
    }

    /// The capacity of the buffer, i.e., how many items it can hold.
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::double_mapped_buffer::pagesize;
    use std::mem;
    use std::sync::atomic::compiler_fence;
    use std::sync::atomic::Ordering;

    #[test]
    fn byte_buffer() {
        let b = DoubleMappedBuffer::<u8>::new(123).expect("failed to create buffer");
        let ps = pagesize();

        assert_eq!(b.capacity() * mem::size_of::<u8>() % ps, 0);
        assert_eq!(b.buffer.addr() % mem::align_of::<u8>(), 0);

        unsafe {
            let s = b.slice_mut();
            assert_eq!(s.len(), b.capacity());
            assert_eq!(s.as_mut_ptr() as usize, b.buffer.addr());

            for (i, v) in s.iter_mut().enumerate() {
                *v = (i % 128) as u8;
            }

            compiler_fence(Ordering::SeqCst);

            let s = b.slice_with_offset(b.capacity());
            assert_eq!(
                s.as_ptr() as usize,
                b.buffer.addr() + b.capacity() * mem::size_of::<u8>()
            );
            for (i, v) in s.iter().enumerate() {
                assert_eq!(*v, (i % 128) as u8);
            }

            compiler_fence(Ordering::SeqCst);
            b.slice_mut()[0] = 123;
            compiler_fence(Ordering::SeqCst);
            assert_eq!(b.slice_with_offset(b.capacity())[0], 123);
        }
    }

    #[test]
    fn u32_buffer() {
        let b = DoubleMappedBuffer::<u32>::new(12311).expect("failed to create buffer");
        let ps = pagesize();

        assert_eq!(b.capacity() * mem::size_of::<u32>() % ps, 0);
        assert_eq!(b.buffer.addr() % mem::align_of::<u32>(), 0);

        unsafe {
            let s = b.slice_mut();
            assert_eq!(s.len(), b.capacity());
            assert_eq!(s.as_mut_ptr() as usize, b.buffer.addr());

            for (i, v) in s.iter_mut().enumerate() {
                *v = (i % 128) as u32;
            }

            compiler_fence(Ordering::SeqCst);

            let s = b.slice_with_offset(b.capacity());
            assert_eq!(
                s.as_ptr() as usize,
                b.buffer.addr() + b.capacity() * mem::size_of::<u32>()
            );
            for (i, v) in s.iter().enumerate() {
                assert_eq!(*v, (i % 128) as u32);
            }

            compiler_fence(Ordering::SeqCst);
            b.slice_mut()[0] = 123;
            compiler_fence(Ordering::SeqCst);
            assert_eq!(b.slice_with_offset(b.capacity())[0], 123);
        }
    }

    #[test]
    fn many_buffers() {
        let _b0 = DoubleMappedBuffer::<u32>::new(123).expect("failed to create buffer");
        let _b1 = DoubleMappedBuffer::<u32>::new(456).expect("failed to create buffer");

        let mut v = Vec::new();

        for _ in 0..100 {
            v.push(DoubleMappedBuffer::<u32>::new(123).expect("failed to create buffer"));
        }
    }
}
