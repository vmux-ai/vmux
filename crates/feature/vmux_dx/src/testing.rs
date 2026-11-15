use std::sync::{Condvar, Mutex};

#[cfg(test)]
static FIXTURES: (Mutex<u8>, Condvar) = (Mutex::new(0), Condvar::new());

#[cfg(test)]
pub(crate) struct Permit;

#[cfg(test)]
impl Permit {
    pub(crate) fn acquire() -> Self {
        let mut active = FIXTURES.0.lock().unwrap();
        while *active >= 4 {
            active = FIXTURES.1.wait(active).unwrap();
        }
        *active += 1;
        Self
    }
}

#[cfg(test)]
impl Drop for Permit {
    fn drop(&mut self) {
        let mut active = FIXTURES.0.lock().unwrap();
        *active -= 1;
        FIXTURES.1.notify_one();
    }
}
