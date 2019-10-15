#![no_std]

use core::cell::{UnsafeCell, Cell};
use core::ops::{Deref, DerefMut};
use core::fmt;

pub struct RwCell<T: ?Sized> {
    count: Cell<RwCount>,
    inner: UnsafeCell<T>,
}
type RwCount = u16;
const RW_WRITE: RwCount = !0;

// TODO handle overflow? if code forget()s a bunch of handles it can kinda go boom...

unsafe impl<T: ?Sized + Send> Send for RwCell<T> { }

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwCell<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut fmt = fmt.debug_struct("RwCell");
        match self.try_read() {
            Some(inner) => {
                fmt.field("inner", &inner);
                fmt.field("borrows", &self.count);
            },
            None => { fmt.field("inner", &"<WRITING>"); },
        }
        fmt.finish()
    }
}

#[cfg(feature = "const-default")]
impl<T: const_default::ConstDefault> const_default::ConstDefault for RwCell<T> {
    const DEFAULT: Self = Self::new(T::DEFAULT);
}

impl<T: ?Sized> RwCell<T> {
    pub const fn new(value: T) -> Self where
        T: Sized
    {
        Self {
            count: Cell::new(0),
            inner: UnsafeCell::new(value),
        }
    }

    unsafe fn inner_mut(&self) -> &mut T {
        &mut *self.inner.get()
    }

    unsafe fn inner(&self) -> &T {
        &*self.inner.get()
    }

    pub fn ptr(&self) -> *const T {
        self.inner.get() as *const _
    }

    pub fn ptr_mut(&self) -> *mut T {
        self.inner.get()
    }

    pub fn try_read<'a>(&'a self) -> Option<RwRead<'a, T>> {
        match self.count.get() {
            RW_WRITE => None,
            count => Some({
                self.count.set(count + 1);
                RwRead(self)
            }),
        }
    }

    pub fn try_read_scope<F: FnOnce(&T) -> R, R>(&self, f: F) -> Option<R> {
        self.try_read().map(|t| f(&*t))
    }

    pub fn try_write<'a>(&'a self) -> Option<RwWrite<'a, T>> {
        match self.count.get() {
            0 => Some({
                self.count.set(RW_WRITE);
                RwWrite(self)
            }),
            _ => None,
        }
    }

    pub fn try_write_scope<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> Option<R> {
        self.try_write().map(|mut t| f(&mut *t))
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe {
            self.inner_mut()
        }
    }

    pub unsafe fn get_ref_unchecked(&self) -> &T {
        self.inner()
    }

    pub unsafe fn get_mut_unchecked(&self) -> &mut T {
        self.inner_mut()
    }
}

pub struct RwRead<'a, T: ?Sized>(&'a RwCell<T>);
pub struct RwWrite<'a, T: ?Sized>(&'a RwCell<T>);

impl<'a, T: ?Sized> RwRead<'a, T> {
    pub fn rw_clone(&self) -> Self {
        let count = self.0.count.get();
        self.0.count.set(count + 1);
        RwRead(self.0)
    }

    // TODO rw_map / rw_map_split
}

impl<'a, T: ?Sized> Drop for RwRead<'a, T> {
    fn drop(&mut self) {
        let count = self.0.count.get();
        debug_assert!(count > 0);
        self.0.count.set(count - 1);
    }
}

impl<'a, T: ?Sized> Drop for RwWrite<'a, T> {
    fn drop(&mut self) {
        let count = self.0.count.replace(0);
        debug_assert!(count == RW_WRITE);
    }
}

impl<'a, T: ?Sized> Deref for RwRead<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe {
            self.0.get_ref_unchecked()
        }
    }
}

impl<'a, T: ?Sized> Deref for RwWrite<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe {
            self.0.get_ref_unchecked()
        }
    }
}

impl<'a, T: ?Sized> DerefMut for RwWrite<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            self.0.get_mut_unchecked()
        }
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for RwRead<'a, T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, fmt)
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for RwWrite<'a, T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, fmt)
    }
}
