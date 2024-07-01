use core::{borrow::Borrow, cell::UnsafeCell, cmp::max};

use crate::sync::blocking_mutex::{Mutex as BlockingMutex, RawMutex};

#[derive(Clone, Copy)]
enum State {
    Owned(u8),
    CoOwned(u8),
    Contended(u8),
}

struct SrpMutex<M, T> {
    // configured: AtomicBool,
    state: BlockingMutex<M, State>,
    inner: UnsafeCell<T>,
}

impl<M: RawMutex, T: Clone> SrpMutex<M, T> {
    pub fn new(prio: u8, value: T) -> Self {
        Self {
            // configured: AtomicBool::new(false),
            state: BlockingMutex::new(State::Owned(prio)),
            inner: UnsafeCell::new(value),
        }
    }

    pub fn register_access(&self, prio: u8) {
        self.state.lock_mut(|x| match *x {
            State::Owned(p) => {
                *x = if p == prio {
                    State::CoOwned(p)
                } else {
                    State::Contended(max(p, prio))
                }
            }
            State::CoOwned(p) => {
                if p != prio {
                    *x = State::Contended(max(p, prio))
                }
            }
            State::Contended(p) => {
                if p != prio {
                    *x = State::Contended(max(p, prio))
                }
            }
        });
    }

    // pub fn lock_registration(&self) {
    //     self.configured.store(true, Ordering::SeqCst);
    // }

    fn do_srp<U>(&self, f: impl FnOnce() -> U) -> U {
        match unsafe {
            // SAFETY: state should not change after configuration
            self.state.dirty_read(|x| *x)
        } {
            State::Contended(p) => {
                // read current ceiling
                use bw_r_drivers_tc37x::pac::CPU0;
                let previous_ceiling = unsafe { CPU0.icr().read().ccpn().get() };
                
                // update ceiling
                if previous_ceiling < p {
                    unsafe {
                        CPU0.icr().modify(|r| r.ccpn().set(p));
                    }
                }

                // execute function
                let x = f();

                // restore ceiling
                unsafe {
                    CPU0.icr().modify(|r| r.ccpn().set(previous_ceiling));
                }

                x
            }
            _ => f(),
        }
    }

    pub fn lock_mut<U>(&self, f: impl FnOnce(&mut T) -> U) -> U {
        self.do_srp(|| {
            let inner_ref_mut = unsafe { &mut *self.inner.get() };
            f(inner_ref_mut)
        })
    }

    pub fn lock<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        self.do_srp(|| {
            let inner_ref = unsafe { &mut *self.inner.get() };
            f(&inner_ref)
        })
    }
}
