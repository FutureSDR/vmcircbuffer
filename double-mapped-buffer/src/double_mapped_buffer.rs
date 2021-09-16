use std::marker::PhantomData;
use std::mem;
use std::slice;

use super::DoubleMappedBufferImpl;
use super::DoubleMappedBufferError;

pub struct DoubleMappedBuffer<T> {
    buffer: DoubleMappedBufferImpl,
    _p: PhantomData<T>,
}

impl<T> DoubleMappedBuffer<T> {

    pub fn new(min_items: usize) -> Result<Self, DoubleMappedBufferError> {
        match DoubleMappedBufferImpl::new(min_items, mem::size_of::<T>(), mem::align_of::<T>()) {
            Ok(buffer) => Ok(DoubleMappedBuffer {buffer, _p: PhantomData}),
            Err(e) => Err(e),
        }
    }

    pub fn slice(&self) -> &[T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        unsafe {
            slice::from_raw_parts(addr as *const T, self.buffer.len())
        }
    }

    pub fn slice_mut(&mut self) -> &mut [T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        unsafe {
            slice::from_raw_parts_mut(addr as *mut T, self.buffer.len())
        }
    }

    pub fn slice_with_offset(&self, offset: usize) -> &[T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        assert!(offset <= self.buffer.len());
        unsafe {
            slice::from_raw_parts((addr as *const T).add(offset), self.buffer.len())
        }
    }

    pub fn slice_mut_with_offset(&mut self, offset: usize) -> &mut [T] {
        let addr = self.buffer.addr() as usize;
        debug_assert_eq!(addr % mem::align_of::<T>(), 0);
        assert!(offset <= self.buffer.len());
        unsafe {
            slice::from_raw_parts_mut((addr as *mut T).add(offset), self.buffer.len())
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
       self.buffer.len()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::mem;
    use std::slice;

    #[test]
    fn tmp_file() {
        let ps = 3 * pagesize();
        let b = DoubleMappedBufferImpl::new(ps);
        assert!(b.is_ok());
        let b = b.unwrap();

        unsafe {
            let b1 =
                slice::from_raw_parts_mut::<u64>(b.addr.cast::<u64>(), ps / mem::size_of::<u64>());
            let b2 = slice::from_raw_parts_mut::<u64>(
                b.addr.add(b.size).cast::<u64>(),
                ps / mem::size_of::<u64>(),
            );
            for (i, v) in b1.iter_mut().enumerate() {
                *v = i as u64;
            }
            assert_eq!(b1, b2);
        }
    }
}
