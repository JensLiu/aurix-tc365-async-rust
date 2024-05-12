use core::{future::Future, marker::PhantomData, pin, task::Poll};

use crate::cpu::Core;

pub struct Yielder<A> {
    yielded: bool,
    _phantom: PhantomData<A>,
}

impl<A: Core> Yielder<A> {
    pub fn new() -> Self {
        Self {
            yielded: false,
            _phantom: PhantomData,
        }
    }
}

impl<A: Core + core::marker::Unpin> Future for Yielder<A> {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        if self.yielded {
            // already yielded once, now exit
            Poll::Ready(())
        } else {
            // yield once
            self.get_mut().yielded = true;

            // wake itself, it will be immidiately scheduled to run the next time
            cx.waker().wake_by_ref();

            // wake up the CPU
            A::signal_event_local();

            Poll::Pending
        }
    }
}