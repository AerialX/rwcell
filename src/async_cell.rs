use core::future::Future;
use core::task::{Poll, Context, Waker};
use core::ops::{Deref, DerefMut};
use core::pin::Pin;
use crate::RwCell;
use wakers::{Wakers, WakerQueue, SendWakers};

pub type AsyncCellWakers = SendWakers<WakerQueue>;

pub struct AsyncCell<T: ?Sized, W = AsyncCellWakers> {
    wakers: W,
    cell: RwCell<T>,
}

unsafe impl<T: ?Sized + Send, W: Send> Send for AsyncCell<T, W> { }

impl<T, W: Default> AsyncCell<T, W> {
    #[inline]
    pub fn new(inner: T) -> Self {
        Self::from_cell(RwCell::new(inner))
    }

    #[inline]
    pub fn from_cell(cell: RwCell<T>) -> Self {
        Self {
            cell,
            wakers: Default::default(),
        }
    }
}

impl<T: ?Sized, W> AsyncCell<T, W> {
    #[inline]
    pub const fn cell_ref(&self) -> &RwCell<T> {
        &self.cell
    }

    #[inline]
    pub fn cell_mut(&mut self) -> &mut RwCell<T> {
        &mut self.cell
    }

    #[inline]
    pub fn async_read(&self) -> AsyncReadFuture<T, W> {
        AsyncReadFuture {
            cell: self,
        }
    }

    #[inline]
    pub fn async_write(&self) -> AsyncWriteFuture<T, W> {
        AsyncWriteFuture {
            cell: self,
        }
    }
}

impl<T: ?Sized, W: Wakers> AsyncCell<T, W> {
    #[inline]
    fn pend(&self, waker: &Waker) {
        self.wakers.pend_by_ref(waker);
    }

    #[inline]
    fn wake(&self) {
        self.wakers.wake_by_ref();
    }
}

impl<T: ?Sized, W> Deref for AsyncCell<T, W> {
    type Target = RwCell<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.cell
    }
}

impl<T: ?Sized, W> DerefMut for AsyncCell<T, W> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cell
    }
}

impl<T: Default, W: Default> Default for AsyncCell<T, W> {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
    }
}

#[cfg(feature = "const-default")]
impl<T: const_default::ConstDefault, W: const_default::ConstDefault> const_default::ConstDefault for AsyncCell<T, W> {
    const DEFAULT: Self = Self {
        cell: const_default::ConstDefault::DEFAULT,
        wakers: const_default::ConstDefault::DEFAULT,
    };
}

pub struct AsyncReadFuture<'a, T: ?Sized, W> {
    cell: &'a AsyncCell<T, W>,
}

pub struct AsyncWriteFuture<'a, T: ?Sized, W> {
    cell: &'a AsyncCell<T, W>,
}

pub struct AsyncRead<'a, T: ?Sized, W: Wakers> {
    cell: &'a AsyncCell<T, W>,
}

pub struct AsyncWrite<'a, T: ?Sized, W: Wakers> {
    cell: &'a AsyncCell<T, W>,
}

impl<'a, T: ?Sized, W: Wakers> Future for AsyncReadFuture<'a, T, W> {
    type Output = AsyncRead<'a, T, W>;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        match self.cell.acquire_read() {
            true => Poll::Ready(AsyncRead {
                cell: self.cell,
            }),
            false => {
                self.cell.pend(context.waker());
                Poll::Pending
            },
        }
    }
}

impl<'a, T: ?Sized, W: Wakers> Future for AsyncWriteFuture<'a, T, W> {
    type Output = AsyncWrite<'a, T, W>;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        match self.cell.acquire_write() {
            true => Poll::Ready(AsyncWrite {
                cell: self.cell,
            }),
            false => {
                self.cell.pend(context.waker());
                Poll::Pending
            },
        }
    }
}

impl<'a, T: ?Sized, W: Wakers> Drop for AsyncRead<'a, T, W> {
    fn drop(&mut self) {
        let _unread = unsafe { self.cell.release_read() };
        debug_assert!(_unread);
        self.cell.wake();
    }
}

impl<'a, T: ?Sized, W: Wakers> Drop for AsyncWrite<'a, T, W> {
    fn drop(&mut self) {
        let _unwrite = unsafe { self.cell.release_write() };
        debug_assert!(_unwrite);
        self.cell.wake();
    }
}

impl<'a, T: ?Sized, W: Wakers> Deref for AsyncRead<'a, T, W> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            self.cell.get_ref_unchecked()
        }
    }
}

impl<'a, T: ?Sized, W: Wakers> Deref for AsyncWrite<'a, T, W> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            self.cell.get_ref_unchecked()
        }
    }
}

impl<'a, T: ?Sized, W: Wakers> DerefMut for AsyncWrite<'a, T, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            self.cell.get_mut_unchecked()
        }
    }
}
