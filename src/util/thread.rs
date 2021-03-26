use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

pub(crate) const OTHER_THREAD_ERR: &str = "This error is shown because other thread has panicked \
                                   You can safety skip this error.";

enum ThreadState<S: Send + 'static> {
    Ready(S),
    Running(JoinHandle<S>, Arc<AtomicBool>),
    Finishing(JoinHandle<S>),
}

/// Thread utility to spawn/finalize/join threads without lossing the state.
pub struct RunnableThread<S: Send + 'static> {
    name: String,
    thread_state: Option<ThreadState<S>>,
}

impl<S: Send + 'static> RunnableThread<S> {
    /// Initialize a thread with a name and a state
    pub fn new(name: &str, state: S) -> Self {
        Self { name: name.into(), thread_state: Some(ThreadState::Ready(state)) }
    }

    fn start_thread(
        name: &str,
        running: Arc<AtomicBool>,
        mut state: S,
        callback: impl Fn(&mut S) + Send + 'static,
    ) -> JoinHandle<S> {
        thread::Builder::new()
            .name(format!("{}/{}", thread::current().name().unwrap_or(""), name))
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    callback(&mut state);
                }
                state
            })
            .unwrap()
    }

    /// Creates a new thread that will continuously call to the callback.
    /// It is in charge of the user to perform a blocking operation there.
    /// If the thread is already running it will returns an `RunningErr` error.
    pub fn spawn(&mut self, callback: impl Fn(&mut S) + Send + 'static) -> Result<(), RunningErr> {
        let thread_state = self.thread_state.take().unwrap();
        let (thread_state, result) = match thread_state {
            ThreadState::Ready(state) => {
                let thread_running = Arc::new(AtomicBool::new(true));
                let running = thread_running.clone();
                let thread = Self::start_thread(&self.name, running, state, callback);

                log::trace!("Thread [{}] spawned", self.name);
                (ThreadState::Running(thread, thread_running), Ok(()))
            }
            _ => (thread_state, Err(RunningErr(self.name.clone()))),
        };
        self.thread_state = Some(thread_state);
        result
    }

    /// Finalizes the thread.
    /// After this call, the current spawn's callback will be last one.
    /// This call do not wait to finish that process,
    /// only notify that the current callback call is the last one.
    /// If you want to wait to finish the job call `RunnableThread::join()`.
    pub fn finalize(&mut self) -> Result<(), NotRunningErr> {
        let thread_state = self.thread_state.take().unwrap();
        let (thread_state, result) = match thread_state {
            ThreadState::Ready(..) => (thread_state, Err(NotRunningErr)),
            ThreadState::Running(thread, running) => {
                log::trace!("Thread [{}] finalized", self.name);
                running.store(false, Ordering::Relaxed);
                (ThreadState::Finishing(thread), Ok(()))
            }
            ThreadState::Finishing(..) => (thread_state, Ok(())),
        };
        self.thread_state = Some(thread_state);
        result
    }

    /// Waits to the thread to finalize (without request to finalize).
    /// This call will block until a `RunningThread::finalize()` was called.
    /// Join a thread not spawned will not wait.
    pub fn join(&mut self) {
        log::trace!("Waiting to finish thread: [{}]", self.name);
        let thread_state = match self.thread_state.take().unwrap() {
            ThreadState::Ready(state) => state,
            ThreadState::Running(thread, _) => thread.join().expect(OTHER_THREAD_ERR),
            ThreadState::Finishing(thread) => thread.join().expect(OTHER_THREAD_ERR),
        };
        self.thread_state = Some(ThreadState::Ready(thread_state));
        log::trace!("Finished to waiting thread: [{}]", self.name);
    }

    /// Check if the thread is running.
    /// A `RunningThread` is considered running if `RunningThread::spawn()` was called but
    /// `RunningThread::finalize()` not.
    pub fn is_running(&self) -> bool {
        match self.thread_state.as_ref().unwrap() {
            ThreadState::Ready(..) => false,
            ThreadState::Running(..) => true,
            ThreadState::Finishing(..) => true,
        }
    }

    /// Consumes the `RunningThread` to recover the state given in its creation.
    /// You only can consume the thread it is totally stoped
    /// (after RunnableThread::join() was called)
    pub fn take_state(mut self) -> Result<S, RunningErr> {
        match self.thread_state.take().unwrap() {
            ThreadState::Ready(state) => Ok(state),
            _ => Err(RunningErr(self.name.clone())),
        }
    }

    /// Read access to the the state given in its creation.
    /// You only can consume the thread it is totally stoped
    /// (after RunnableThread::join() was called)
    pub fn state_ref(&self) -> Result<&S, RunningErr> {
        match self.thread_state.as_ref().unwrap() {
            ThreadState::Ready(state) => Ok(state),
            _ => Err(RunningErr(self.name.clone())),
        }
    }

    /// Mutable access to the the state given in its creation.
    /// You only can consume the thread it is totally stoped
    /// (after RunnableThread::join() was called)
    pub fn state_mut(&mut self) -> Result<&mut S, RunningErr> {
        match self.thread_state.as_mut().unwrap() {
            ThreadState::Ready(state) => Ok(state),
            _ => Err(RunningErr(self.name.clone())),
        }
    }
}

impl<S: Send> Drop for RunnableThread<S> {
    fn drop(&mut self) {
        if self.thread_state.is_some() && self.finalize().is_ok() {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotRunningErr;

impl std::error::Error for NotRunningErr {}

impl std::fmt::Display for NotRunningErr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "The action requires that the thread is running")
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
        assert_eq!(Ok(&42), thread.state_ref());
        assert_eq!(Ok(&mut 42), thread.state_mut());

        thread
            .spawn(|internal_state| {
                assert_eq!(&mut 42, internal_state);
                *internal_state = 123;
                std::thread::sleep(*STEP_DURATION);
            })
            .unwrap();
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.state_ref());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.state_mut());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.spawn(|_| ()));
        assert!(thread.is_running());

        std::thread::sleep(*STEP_DURATION / 2);
        thread.finalize().unwrap();
        assert!(thread.is_running()); // Continuous running after finalize()
        assert_eq!(Ok(()), thread.finalize());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.state_ref());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.state_mut());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.spawn(|_| ()));

        std::thread::sleep(*STEP_DURATION);
        assert!(thread.is_running()); // Continuous running after end operation
        assert_eq!(Ok(()), thread.finalize());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.state_ref());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.state_mut());
        assert_eq!(Err(RunningErr(UT_THREAD_NAME.into())), thread.spawn(|_| ()));

        thread.join();
        assert!(!thread.is_running());
        assert_eq!(Ok(&123), thread.state_ref());
        assert_eq!(Ok(&mut 123), thread.state_mut());
        assert_eq!(Err(NotRunningErr), thread.finalize());
        thread.join(); //nothing happens

        assert_eq!(Ok(123), thread.take_state()); //consume the thread
    }

    #[test]
    fn spawn_and_spawn_again() {
        let mut thread = RunnableThread::new(UT_THREAD_NAME, ());
        thread.spawn(|_| ()).unwrap();
        thread.finalize().unwrap();
        thread.join();
        thread.spawn(|_| ()).unwrap();
        assert!(thread.is_running());
        thread.finalize().unwrap();
        thread.join();
    }

    #[test]
    fn destroy_while_running() {
        let mut thread = RunnableThread::new(UT_THREAD_NAME, ());
        thread.spawn(|_| ()).unwrap();
        drop(thread)
    }

    #[test]
    fn destroy_while_finalizing() {
        let mut thread = RunnableThread::new(UT_THREAD_NAME, ());
        thread.spawn(|_| ()).unwrap();
        thread.finalize().unwrap();
        drop(thread)
    }
}
