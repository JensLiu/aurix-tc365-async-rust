use core::{
    mem,
    task::{RawWaker, RawWakerVTable, Waker},
};

use bw_r_drivers_tc37x::{gpio::GpioExt, pac};

use crate::{executor::task::{TaskHeader, TaskRef}, print};

// waker context, p is a raw pointer to an atomic bool variable
pub(crate) static VTABLE: RawWakerVTable = {
    unsafe fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }

    unsafe fn wake(p: *const ()) {
        wake_by_ref(p);
    }

    unsafe fn wake_by_ref(p: *const ()) {
        // let gpio00 = pac::P00.split();
        // let mut pin2 = gpio00.p00_2.into_push_pull_output();
        // pin2.set_high();
        
        // ~100ns
        let task = TaskRef::from_ptr(p as *const TaskHeader).as_static_mut_header();
        // print!("Waker::wake_by_ref: wake task {}\n", task.name);
        task.executor.enqueue_and_pend(task);
    }

    unsafe fn drop(_: *const ()) {
        // no-op
    }

    RawWakerVTable::new(clone, wake, wake_by_ref, drop)
};

// for now, we cannot extract data directly from the waker API in stable Rust
struct WakerHack {
    data: *const (),
    vtable: &'static RawWakerVTable,
}

pub fn task_from_waker(waker: &Waker) -> TaskRef {
    // safety: OK because WakerHack has the same layout as Waker.
    // This is not really guaranteed because the structs are `repr(Rust)`, it is
    // indeed the case in the current implementation.
    // TODO use waker_getters when stable. https://github.com/rust-lang/rust/issues/96992
    let hack: &WakerHack = unsafe { mem::transmute(waker) };
    if hack.vtable != &VTABLE {
        panic!("Found waker not created by the Embassy executor. `embassy_time::Timer` only works with the Embassy executor.")
    }

    // safety: our wakers are always created with `TaskRef::as_ptr`
    unsafe { TaskRef::from_ptr(hack.data as *const TaskHeader) }
}
