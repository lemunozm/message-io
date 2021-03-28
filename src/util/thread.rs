use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

pub(crate) const OTHER_THREAD_ERR: &str = "This error is shown because other thread has panicked \
                                   You can safety skip this error.";

enum ThreadState<S: Send + 'static> {
    Ready(S),
    Running(JoinHandle<S>),
}

/// Shareable handler of the thread.
/// It contains the safer shareable methods of a [`RunnableThread`]
pub struct ThreadHandler {
    name: String,
    running: Arc<AtomicBool>,
}

impl ThreadHandler {
    pub fn new(name: String, initialized: bool) -> Self {
        let handler = Self {name, running: Arc::new(AtomicBool::new(false))};
        if initialized {
            handler.spawned();
        }
        handler
    }

    /// Assumes that the thread is initialized.
    fn spawned(&self) {
        self.running.store(true, Ordering::Relaxed);
        log::trace!("Thread [{}] spawned", self.name);
    }

    /// Check if the thread is running.
    /// A `RunningThread` is considered running if [`RunningThread::spawn()`] was called
    /// but [`RunningThread::finalize()`] not.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Finalizes the thread.
    /// After this call, the thread is considered not running although the last
    /// callback job could be processing.
    /// This call do not wait to finish that job.
    /// If you want to wait to finish it, call [`RunnableThread::join()`].
    pub fn finalize(&self) {
        self.running.store(false, Ordering::Relaxed);
        log::trace!("Thread [{}] finalized", self.name);
    }
}

impl Clone for ThreadHandler {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            running: self.running.clone(),
        }
    }
}


/// Thread utility to spawn/finalize/join threads without lossing the state.
pub struct RunnableThread<S: Send + 'static> {
    thread_state: Option<ThreadState<S>>,
    handler: ThreadHandler,
}

impl<S: Send + 'static> RunnableThread<S> {
    /// Initialize a thread with a name and a state
    pub fn new(name: &str, state: S) -> Self {
        Self {
            thread_state: Some(ThreadState::Ready(state)),
            handler: ThreadHandler::new(name.into(), false),
        }
    }

    fn start_thread(
        handler: ThreadHandler,
        mut state: S,
        mut callback: impl FnMut(&mut S, &ThreadHandler) + Send + 'static,
    ) -> JoinHandle<S> {
        handler.spawned();
        thread::Builder::new()
            .name(format!("{}/{}", thread::current().name().unwrap_or(""), handler.name))
            .spawn(move || {
                while handler.is_running() {
                    callback(&mut state, &handler);
                }
                state
            })
            .unwrap()
    }

    /// Creates a new thread that will continuously call to the callback.
    /// It is in charge of the user to perform a blocking operation in the callback.
    /// If the thread is already running it will returns an [`RunningErr`] error.
    pub fn spawn(
        &mut self,
        callback: impl FnMut(&mut S, &ThreadHandler) + Send + 'static
    ) -> Result<(), RunningErr> {
        if !self.handler.is_running() {
            self.join(); // Ensure the processing is finished.
        }

        let thread_state = self.thread_state.take().unwrap();
        match thread_state {
            ThreadState::Ready(state) => {
                let thread = Self::start_thread(self.handler.clone(), state, callback);
                self.thread_state = Some(ThreadState::Running(thread));
                Ok(())
            }
            ThreadState::Running(..) => {
                self.thread_state = Some(thread_state);
                Err(RunningErr(self.handler.name.clone()))
            }
        }
    }

    /// Waits to the thread to finalize.
    /// This call will block until a `RunningThread::finalize()` was called.
    /// Join a thread not spawned will not wait.
    pub fn join(&mut self) {
        log::trace!("Waiting to finish thread: [{}]", self.handler.name);
        let thread_state = match self.thread_state.take().unwrap() {
            ThreadState::Ready(state) => ThreadState::Ready(state),
            ThreadState::Running(thread) => {
                ThreadState::Ready(thread.join().expect(OTHER_THREAD_ERR))
            },
        };
        self.thread_state = Some(thread_state);
        log::trace!("Finished to waiting thread: [{}]", self.handler.name);
    }

    /// Consumes the `RunningThread` to recover the state given in its creation.
    /// You only can consume finalized or not spawned threads.
    pub fn take_state(mut self) -> Result<S, RunningErr> {
        if !self.handler.is_running() {
            self.join(); // Ensure the processing is finished.
        }

        let thread_state = self.thread_state.take().unwrap();
        match thread_state {
            ThreadState::Ready(state) => Ok(state),
            ThreadState::Running(..) => {
                self.thread_state = Some(thread_state);
                Err(RunningErr(self.handler.name.clone()))
            }
        }
    }

    /// Mutable access to the state given in its creation.
    /// You only can request the state if the thread is not running.
    pub fn state_mut(&mut self) -> Result<&mut S, RunningErr> {
        if !self.handler.is_running() {
            self.join(); // Ensure the processing is finished.
        }

        match self.thread_state.as_mut().unwrap() {
            ThreadState::Ready(state) => Ok(state),
            ThreadState::Running(..) => Err(RunningErr(self.handler.name.clone())),
        }
    }

    pub fn handler(&self) -> &ThreadHandler {
        &self.handler
    }
}

impl<S: Send> Drop for RunnableThread<S> {
    fn drop(&mut self) {
        if self.thread_state.is_some() {
            self.handler.finalize();
            self.join();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningErr(String);

impl std::error::Error for RunningErr {}

impl std::fmt::Display for RunningErr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "The action requires that the thread [{}] was not running or be finalized",
            self.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration};

    const UT_THREAD_NAME: &str = "message-io::thread::UT";
    lazy_static::lazy_static! {
        static ref STEP_DURATION: Duration = Duration::from_millis(1000);
    }

    #[test]
    fn basic_pipeline() {
        let state = 42;

        let mut thread = RunnableThread::new(UT_THREAD_NAME, state);
        assert_eq!(Ok(&mut 42), thread.state_mut());

        thread
            .spawn(|internal_state, _| {
                assert_eq!(&mut 42, internal_state);
                *internal_state = 123;
                std::thread::sleep(*STEP_DURATION);
            })
            .unwrap();

        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.state_mut());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.spawn(|_, _| ()));
        assert!(thread.handler().is_running());

        std::thread::sleep(*STEP_DURATION / 2);
        thread.handler().finalize();
        assert!(!thread.handler().is_running());
        assert_eq!(Ok(&mut 123), thread.state_mut());
        thread.handler().finalize(); // Nothing happens

        thread.join();

        assert_eq!(Ok(123), thread.take_state()); //consume the thread
    }

    #[test]
    fn stopped_from_callback() {
        let mut thread = RunnableThread::new(UT_THREAD_NAME, ());
        assert_eq!(Ok(()), thread.spawn(|_, handler| {
            std::thread::sleep(*STEP_DURATION);
            handler.finalize() // stopped
        }));
        assert!(thread.handler().is_running());
        thread.join();
        assert!(!thread.handler().is_running());
    }

    #[test]
    fn spawn_and_spawn_again() {
        let mut thread = RunnableThread::new(UT_THREAD_NAME, ());
        assert_eq!(Ok(()), thread.spawn(|_, _| ()));
        thread.handler().finalize();
        assert!(!thread.handler().is_running());

        assert_eq!(Ok(()), thread.spawn(|_, _| ()));
        assert!(thread.handler().is_running());
        thread.handler().finalize();
        assert!(!thread.handler().is_running());
        thread.join();
    }

    #[test]
    fn destroy_while_finalizing() {
        let mut thread = RunnableThread::new(UT_THREAD_NAME, ());
        assert_eq!(Ok(()), thread.spawn(|_, _| ()));
        thread.handler().finalize();
    }

    #[test]
    fn destroy_while_running() {
        let mut thread = RunnableThread::new(UT_THREAD_NAME, ());
        assert_eq!(Ok(()), thread.spawn(|_, _| ()));
    }
}
