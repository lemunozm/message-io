use std::thread::{self, JoinHandle};

/// A comprensive error message to notify that the error shown is from other thread.
pub const OTHER_THREAD_ERR: &str = "Avoid this 'panicked_at' error. \
                                   This error is shown because other thread has panicked \
                                   You can safety skip this error.";

/// Thread similar to the std, but with a name that can be nested.
pub struct NamespacedThread<T: Send + 'static> {
    namespace: String,
    join_handle: Option<JoinHandle<T>>,
}

impl<T: Send + 'static> NamespacedThread<T> {
    /// Similar to [`thread::spawn()`] but with a name.
    pub fn spawn<F>(name: &str, f: F) -> Self
    where
        F: FnOnce() -> T,
        F: Send + 'static,
    {
        let namespace = format!("{}/{}", thread::current().name().unwrap_or(""), name);
        Self {
            namespace: namespace.clone(),
            join_handle: Some(
                thread::Builder::new()
                    .name(namespace.clone())
                    .spawn(move || {
                        log::trace!("Thread [{}] spawned", namespace);
                        f()
                    })
                    .unwrap(),
            ),
        }
    }

    /// Wait the thread to finish.
    pub fn join(&mut self) -> T {
        log::trace!("Join thread [{}] ...", self.namespace);
        let content = self.join_handle.take().unwrap().join().expect(OTHER_THREAD_ERR);
        log::trace!("Joined thread [{}]", self.namespace);
        content
    }

    /// Wait the thread to finish.
    /// Returns the inner `T` value if never was joined, `None` otherwise
    pub fn try_join(&mut self) -> Option<T> {
        if self.join_handle.is_some() {
            return Some(self.join());
        }
        None
    }
}

impl<T: Send + 'static> Drop for NamespacedThread<T> {
    fn drop(&mut self) {
        self.try_join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc};

    #[test]
    fn basic_usage() {
        let called = Arc::new(AtomicBool::new(false));
        let mut thread = {
            let called = called.clone();
            NamespacedThread::spawn("test", move || {
                std::thread::sleep(Duration::from_millis(500));
                called.store(true, Ordering::Relaxed);
            })
        };

        std::thread::sleep(Duration::from_millis(250));
        assert!(!called.load(Ordering::Relaxed));
        std::thread::sleep(Duration::from_millis(500));
        assert!(called.load(Ordering::Relaxed));
        thread.join();
    }

    #[test]
    fn join_result() {
        let called = Arc::new(AtomicBool::new(false));
        let mut thread = {
            let called = called.clone();
            NamespacedThread::spawn("test", move || {
                std::thread::sleep(Duration::from_millis(500));
                called.store(true, Ordering::Relaxed);
                "result"
            })
        };
        assert_eq!("result", thread.join());
        assert!(called.load(Ordering::Relaxed));
    }

    #[test]
    fn drop_implies_join() {
        let called = Arc::new(AtomicBool::new(false));
        let thread = {
            let called = called.clone();
            NamespacedThread::spawn("test", move || {
                std::thread::sleep(Duration::from_millis(500));
                called.store(true, Ordering::Relaxed);
            })
        };
        drop(thread);
        assert!(called.load(Ordering::Relaxed));
    }
}
