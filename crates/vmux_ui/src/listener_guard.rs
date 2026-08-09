use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(crate) struct ListenerGuard {
    active: Rc<Cell<bool>>,
}

impl Clone for ListenerGuard {
    fn clone(&self) -> Self {
        Self {
            active: Rc::clone(&self.active),
        }
    }
}

impl ListenerGuard {
    pub(crate) fn new() -> Self {
        Self {
            active: Rc::new(Cell::new(true)),
        }
    }

    pub(crate) fn deactivate(&self) {
        self.active.set(false);
    }

    fn is_active(&self) -> bool {
        self.active.get()
    }
}

pub(crate) struct GuardedListener<F> {
    guard: ListenerGuard,
    callback: Rc<RefCell<F>>,
}

impl<F> Clone for GuardedListener<F> {
    fn clone(&self) -> Self {
        Self {
            guard: self.guard.clone(),
            callback: Rc::clone(&self.callback),
        }
    }
}

impl<F> GuardedListener<F> {
    pub(crate) fn new(callback: F) -> Self {
        Self {
            guard: ListenerGuard::new(),
            callback: Rc::new(RefCell::new(callback)),
        }
    }

    pub(crate) fn guard(&self) -> ListenerGuard {
        self.guard.clone()
    }

    pub(crate) fn call<T>(&self, value: T) -> bool
    where
        F: FnMut(T),
    {
        if !self.guard.is_active() {
            return false;
        }
        let Ok(mut callback) = self.callback.try_borrow_mut() else {
            return false;
        };
        callback(value);
        true
    }
}

#[cfg(test)]
#[path = "listener_guard.test.rs"]
mod tests;
