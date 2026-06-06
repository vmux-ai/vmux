use std::marker::PhantomData;
use std::os::raw::c_void;

#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    fn IOSurfaceIncrementUseCount(buffer: *const c_void);
    fn IOSurfaceDecrementUseCount(buffer: *const c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

pub trait IoSurfaceOps {
    fn acquire(io_surface: *mut c_void);
    fn release(io_surface: *mut c_void);
}

pub enum RealIoSurfaceOps {}

impl IoSurfaceOps for RealIoSurfaceOps {
    fn acquire(io_surface: *mut c_void) {
        unsafe {
            CFRetain(io_surface);
            IOSurfaceIncrementUseCount(io_surface);
        }
    }

    fn release(io_surface: *mut c_void) {
        unsafe {
            IOSurfaceDecrementUseCount(io_surface);
            CFRelease(io_surface);
        }
    }
}

pub struct IoSurfaceKeepAlive<O: IoSurfaceOps = RealIoSurfaceOps> {
    io_surface: *mut c_void,
    _ops: PhantomData<O>,
}

impl<O: IoSurfaceOps> IoSurfaceKeepAlive<O> {
    pub fn retain(io_surface: *mut c_void) -> Self {
        O::acquire(io_surface);
        Self {
            io_surface,
            _ops: PhantomData,
        }
    }
}

impl<O: IoSurfaceOps> Drop for IoSurfaceKeepAlive<O> {
    fn drop(&mut self) {
        O::release(self.io_surface);
    }
}

unsafe impl<O: IoSurfaceOps> Send for IoSurfaceKeepAlive<O> {}
unsafe impl<O: IoSurfaceOps> Sync for IoSurfaceKeepAlive<O> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ACQUIRES: AtomicUsize = AtomicUsize::new(0);
    static RELEASES: AtomicUsize = AtomicUsize::new(0);

    enum TestOps {}

    impl IoSurfaceOps for TestOps {
        fn acquire(_io_surface: *mut c_void) {
            ACQUIRES.fetch_add(1, Ordering::SeqCst);
        }

        fn release(_io_surface: *mut c_void) {
            RELEASES.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn retain_acquires_once_and_drop_releases_once() {
        ACQUIRES.store(0, Ordering::SeqCst);
        RELEASES.store(0, Ordering::SeqCst);
        {
            let _keep = IoSurfaceKeepAlive::<TestOps>::retain(std::ptr::null_mut());
            assert_eq!(ACQUIRES.load(Ordering::SeqCst), 1);
            assert_eq!(RELEASES.load(Ordering::SeqCst), 0);
        }
        assert_eq!(RELEASES.load(Ordering::SeqCst), 1);
    }
}
