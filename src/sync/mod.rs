// This mod MUST go first, so that the others see its macros.
pub(crate) mod fmt;



// internal use
mod ring_buffer;

#[allow(unused)]
pub mod blocking_mutex;
#[allow(unused)]
pub mod channel;
#[allow(unused)]
pub mod mutex;
#[allow(unused)]
pub mod once_lock;
#[allow(unused)]
pub mod pipe;
#[allow(unused)]
pub mod priority_channel;
#[allow(unused)]
pub mod pubsub;
#[allow(unused)]
pub mod signal;
#[allow(unused)]
pub mod waitqueue;
#[allow(unused)]
pub mod zerocopy_channel;
#[allow(unused)]
pub mod select;