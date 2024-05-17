use core::{future::Future, task::Poll};

use crate::{
    cpu::{cpu0::Cpu0, Cpu},
    executor::waker,
};

pub struct Timer {
    expires_at: u64,
}

impl Timer {
    pub fn after_ticks(ticks: u64) -> Self {
        Self {
            expires_at: Cpu0::now() + ticks,
        }
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        if self.expires_at <= Cpu0::now() {
            Poll::Ready(())
        } else {
            // schedule wake up
            unsafe {
                // SAFETY: when the executor calls poll on this future, its top-level furure, i.e. TaskHeader
                // has already been popped from the run_queue
                let header = waker::task_from_waker(cx.waker()).as_static_mut_header();
                header.expires_at = Some(self.expires_at);
                // header
                //     .executor
                //     .unwrap()
                //     .enqueue_timer(self.expires_at, cx.waker().clone());
            }
            Poll::Pending
        }
    }
}
