pub mod generic;

#[cfg(feature = "async")]
pub mod asynchronous;
pub mod sync;
pub mod nonblocking;
