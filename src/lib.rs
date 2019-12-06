#![cfg_attr(feature = "unstable", feature(const_fn))]
#![no_std]

use unchecked_ops::*;
use core::cell::{UnsafeCell, Cell};
use core::ops::{Deref, DerefMut};
use core::fmt;

#[cfg(feature = "async")]
pub mod async_cell;
#[cfg(feature = "async")]
pub type AsyncCell<T> = async_cell::AsyncCell<T, async_cell::AsyncCellWakers>;

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

impl<T: Default> Default for RwCell<T> {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
    }
}

#[cfg(feature = "const-default")]
impl<T: const_default::ConstDefault> const_default::ConstDefault for RwCell<T> {
    const DEFAULT: Self = Self::new(T::DEFAULT);
}

impl<T: ?Sized> RwCell<T> {
    #[inline]
    pub const fn new(value: T) -> Self where
        T: Sized
    {
        Self {
            count: Cell::new(0),
            inner: UnsafeCell::new(value),
        }
    }

    #[inline]
    unsafe fn inner_mut(&self) -> &mut T {
        &mut *self.inner.get()
    }

    #[inline]
    unsafe fn inner(&self) -> &T {
        &*self.inner.get()
    }

    fn acquire_read(&self) -> bool {
        match self.count.get().checked_add(1) {
            None => false,
            #[cfg(debug_assertions)]
            Some(RW_WRITE) => false,
            Some(count) => {
                self.count.set(count);
                true
            },
        }
    }

    unsafe fn release_read(&self) -> bool {
        match self.count.get() {
            #[cfg(debug_assertions)]
            0 => false,
            RW_WRITE => false, // NOTE: it's possible to poison a cell by hitting the max read limit
            count => {
                self.count.set(count.unchecked_sub(1));
                true
            },
        }
    }

    fn acquire_write(&self) -> bool {
        match self.count.get() {
            0 => {
                self.count.set(RW_WRITE);
                true
            },
            _ => false,
        }
    }

    #[inline]
    unsafe fn release_write(&self) -> bool {
        let count = self.count.replace(0);
        count == RW_WRITE
    }

    pub fn readers(&self) -> Option<RwCount> {
        match self.count.get() {
            RW_WRITE => None,
            count => Some(count),
        }
    }

    #[inline]
    pub fn ptr(&self) -> *const T {
        self.inner.get() as *const _
    }

    #[inline]
    pub fn ptr_mut(&self) -> *mut T {
        self.inner.get()
    }

    #[inline]
    pub fn try_read<'a>(&'a self) -> Option<RwRead<'a, T>> {
        match self.acquire_read() {
            true => Some(RwRead(self)),
            false => None,
        }
    }

    #[inline]
    pub fn try_read_scope<F: FnOnce(&T) -> R, R>(&self, f: F) -> Option<R> {
        self.try_read().map(|t| f(&*t))
    }

    #[inline]
    pub fn try_write<'a>(&'a self) -> Option<RwWrite<'a, T>> {
        match self.acquire_write() {
            true => Some(RwWrite(self)),
            false => None,
        }
    }

    #[inline]
    pub fn try_write_scope<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> Option<R> {
        self.try_write().map(|mut t| f(&mut *t))
    }

    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        unsafe {
            self.inner_mut()
        }
    }

    #[inline]
    pub unsafe fn get_ref_unchecked(&self) -> &T {
        self.inner()
    }

    #[inline]
    pub unsafe fn get_mut_unchecked(&self) -> &mut T {
        self.inner_mut()
    }
}

pub struct RwRead<'a, T: ?Sized>(&'a RwCell<T>);
pub struct RwWrite<'a, T: ?Sized>(&'a RwCell<T>);

impl<'a, T: ?Sized> RwRead<'a, T> {
    pub fn rw_clone(&self) -> Self {
        let _read = self.0.acquire_read();
        debug_assert!(_read);
        RwRead(self.0)
    }

    // TODO rw_map / rw_map_split
}

impl<'a, T: ?Sized> Drop for RwRead<'a, T> {
    #[inline]
    fn drop(&mut self) {
        let _unread = unsafe { self.0.release_read() };
        debug_assert!(_unread);
    }
}

impl<'a, T: ?Sized> Drop for RwWrite<'a, T> {
    #[inline]
    fn drop(&mut self) {
        let _unwrite = unsafe { self.0.release_write() };
        debug_assert!(_unwrite);
    }
}

impl<'a, T: ?Sized> Deref for RwRead<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            self.0.get_ref_unchecked()
        }
    }
}

impl<'a, T: ?Sized> Deref for RwWrite<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            self.0.get_ref_unchecked()
        }
    }
}

impl<'a, T: ?Sized> DerefMut for RwWrite<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            self.0.get_mut_unchecked()
        }
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for RwRead<'a, T> {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, fmt)
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for RwWrite<'a, T> {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, fmt)
    }
}
