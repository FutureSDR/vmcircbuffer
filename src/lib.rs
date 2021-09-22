//! Double Mapped Circular Buffer
//!
//! - Thread-safe.
//! - Supports multiple readers.
//! - Generic over the item type.
//! - Provides access to all items (not n-1).
//! - Supports Linux, macOS, Windows, and Android.
//! - [Sync](sync), [async](asynchronous), and [non-blocking](nonblocking) implementations.
//! - [Generic](crate::generic) variant that allows specifying custom [Notifiers](crate::generic::Notifier) to ease integration.
//! - Underlying data sturcture (i.e., [DoubleMappedBuffer](double_mapped_buffer::DoubleMappedBuffer)) is exported to allow custom implementations.
//!
//! # Quick Start
//!
//! ```
//! # use vmcircbuffer::sync;
//! # use vmcircbuffer::generic::CircularError;
//! let mut w = sync::Circular::new::<u32>()?;
//! let mut r = w.add_reader();
//!
//! // delay producing by 1 sec
//! let now = std::time::Instant::now();
//! let delay = std::time::Duration::from_millis(1000);
//!
//! // producer thread
//! std::thread::spawn(move || {
//!     std::thread::sleep(delay);
//!     let w_buff = w.slice();
//!     for v in w_buff.iter_mut() {
//!         *v = 23;
//!     }
//!     let l = w_buff.len();
//!     w.produce(l);
//! });
//!
//! // blocks until data becomes available
//! let r_buff = r.slice().unwrap();
//! assert!(now.elapsed() > delay);
//! for v in r_buff {
//!     assert_eq!(*v, 23);
//! }
//! let l = r_buff.len();
//! r.consume(l);
//! # Ok::<(), CircularError>(())
//! ```
//!
//! # Commonalities
//!
//! There are some commonalities between the implementations:
//! - The `Circular` struct is a factory to create the `Writer`.
//! - If there are no `Reader`s, the `Writer` will not block but continuously overwrite the buffer.
//! - The `Writer` has an `add_reader()` method to add `Reader`s.
//! - When the `Writer` is dropped, the `Reader` can read the remaining items. Afterwards, the `slice()` will return `None`.
//!
//! # Details
//!
//! This circular buffer implementation maps the underlying buffer twice,
//! back-to-back into the virtual address space of the process. This arrangement
//! allows the circular buffer to present the available data sequentially,
//! (i.e., as a slice) without having to worry about wrapping.
//!
//! On Unix-based systems, the mapping is setup with a temporary file. This file
//! is created in the folder, determined through [std::env::temp_dir], which
//! considers environment variables. This can be used, if the standard paths are
//! not present of not writable on the platform.
//!
//! # Features
//!
//! The `async` feature flag allows to enable/disable the async implementation. It is enabled by default.

#[cfg(feature = "async")]
pub mod asynchronous;
pub mod double_mapped_buffer;
pub mod generic;
pub mod nonblocking;
pub mod sync;
