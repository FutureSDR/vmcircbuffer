//! Double Mapped Circular Buffer
//!
//! Main features:
//! - Supports multiple readers.
//! - Generic over the item type.
//! - Sync/Async/Nonblocking implementation.
//! - [Generic](crate::generic) implementation allows to specify custom [Notifiers](crate::generic::Notifier) to
//! The crates comes with a [blocking/sync](sync) and [async](asynchronous) implementation. Both will block/await until space becomes available in the buffer.
//! There is also a [nonblocking](nonblocking::Circular) implementation that can be used with a separate
//!
//!
//!
//! # Quick Start
//!
//!``` rust
//!let w = Circular::new::<u32>().unwrap();
//!let r = w.add_reader();
//!
//!for v in w.slice() {
//!    *v = 123;
//!}
//!w.produce(w.slice().len());
//!
//!for v in r.slice().unwrap() {
//!    assert_eq!(*v, 123);
//!}
//!```
//!
//! # Features
//!
//! The `async` feature flag allows to enable/disable the async implementation. It is enabled by default.

#[cfg(feature = "async")]
pub mod asynchronous;
/// Underlying data structure that maps a buffer twice into virtual memory.
pub mod double_mapped_buffer;
/// Circular Buffer with generic wait/block behavior..
pub mod generic;
/// Nonblocking Circular Buffer that can be used with custom
pub mod nonblocking;
/// Blocking/Sync Circular Buffer
pub mod sync;
