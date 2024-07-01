use core::{future::Future, marker::PhantomData, task::Poll};

use bw_r_drivers_tc37x::{gpio::GpioExt, pac};

use crate::{cpu::Cpu, print};

pub struct Yielder<A> {
    yielded: bool,
    _phantom: PhantomData<A>,
}

impl<A: Cpu> Yielder<A> {
    pub fn new() -> Self {
        Self {
            yielded: false,
            _phantom: PhantomData,
        }
    }
}

impl<A: Cpu + core::marker::Unpin> Future for Yielder<A> {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        // let gpio00 = pac::P00.split();
        // let mut pin2 = gpio00.p00_2.into_push_pull_output();
        // pin2.set_high();

        if self.yielded {
            // already yielded once, now exit
            // pin2.set_low();
            Poll::Ready(())
        } else {
            // yield once
            self.get_mut().yielded = true;

            // wake itself, it will be immidiately scheduled to run the next time
            cx.waker().wake_by_ref();

            Poll::Pending
        }
    }
}
