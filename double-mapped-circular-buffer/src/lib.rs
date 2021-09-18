pub mod generic;

#[cfg(feature = "async")]
pub mod asynchronous;
pub mod nonblocking;
pub mod sync;
