// borrowed from Embassy

use core::cell::UnsafeCell;

/// "raw" in a way that it does not contain the protected data
/// It is just implements the mutex mechanism
pub unsafe trait RawMutex {
    /// returns a self
    const INIT: Self;

    /// locks the mutex
    fn lock<R>(&self, f: impl FnOnce() -> R) -> R;
}

pub struct CriticalSectionRawMutex {}

unsafe impl Send for CriticalSectionRawMutex {}
unsafe impl Sync for CriticalSectionRawMutex {}

impl CriticalSectionRawMutex {
    /// Create a new `CriticalSectionRawMutex`.
    pub const fn new() -> Self {
        Self {}
    }
}

unsafe impl RawMutex for CriticalSectionRawMutex {
    const INIT: Self = Self::new();

    fn lock<R>(&self, f: impl FnOnce() -> R) -> R {
        critical_section::with(|_| f())
    }
}

pub struct Mutex<R, T: ?Sized> {
    raw: R,
    data: UnsafeCell<T>,
}

// SAFETY: the RawMutex implementation should guarantee that it can be Send and Sync
unsafe impl<M: RawMutex + Send, T: ?Sized + Send> Send for Mutex<M, T> {}
unsafe impl<M: RawMutex + Sync, T: ?Sized + Send> Sync for Mutex<M, T> {}

impl<R: RawMutex, T> Mutex<R, T> {
    pub const fn new(val: T) -> Self {
        Self {
            raw: R::INIT,
            data: UnsafeCell::new(val),
        }
    }

    pub fn lock<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        self.raw.lock(|| {
            let ptr = self.data.get() as *const T;
            let inner = unsafe { &*ptr };
            f(inner)
        })
    }

    pub fn lock_mut<U>(&self, f: impl FnOnce(&mut T) -> U) -> U {
        let ptr = self.data.get() as *mut T;
        let inner = unsafe { &mut *ptr };
        f(inner)
    }
}
