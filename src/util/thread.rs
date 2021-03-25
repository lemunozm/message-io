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

/// Thread utility to spawn/terminate/join threads without lossing the state.
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
        callback: impl Fn(&mut S, &ThreadRunningIndicator) + Send + 'static,
    ) -> JoinHandle<S> {
        thread::Builder::new()
            .name(format!("{}/{}", thread::current().name().unwrap_or(""), name))
            .spawn(move || {
                let indicator = &ThreadRunningIndicator { running: running.clone() };
                while running.load(Ordering::Relaxed) {
                    callback(&mut state, indicator);
                }
                state
            })
            .unwrap()
    }

    /// Creates a new thread that will continuously call to the callback.
    /// It is in charge of the user to perform a blocking operation there.
    /// If the thread is already running it will returns an `RunningErr` error.
    pub fn spawn(
        &mut self,
        callback: impl Fn(&mut S, &ThreadRunningIndicator) + Send + 'static,
    ) -> Result<(), RunningErr> {
        let state = self.thread_state.take().unwrap();
        let (state, result) = match state {
            ThreadState::Ready(state) => {
                let thread_running = Arc::new(AtomicBool::new(true));
                let running = thread_running.clone();
                let thread = Self::start_thread(&self.name, running, state, callback);

                log::trace!("Thread [{}] spawned", self.name);
                (ThreadState::Running(thread, thread_running), Ok(()))
            }
            _ => (state, Err(RunningErr(self.name.clone()))),
        };
        self.thread_state = Some(state);
        result
    }

    /// Terminates the thread.
    /// After this call, the current spawn's callback will be last one.
    /// This call do not wait to finish that process,
    /// only notify that the current callback call is the last one.
    /// If you want to wait to finish the job call `RunnableThread::join()`.
    pub fn terminate(&mut self) -> Result<(), NotRunningErr> {
        let state = self.thread_state.take().unwrap();
        let (state, result) = match state {
            ThreadState::Ready(..) => (state, Err(NotRunningErr)),
            ThreadState::Running(thread, running) => {
                log::trace!("Thread [{}] terminated", self.name);
                running.store(false, Ordering::Relaxed);
                (ThreadState::Finishing(thread), Ok(()))
            }
            ThreadState::Finishing(..) => (state, Ok(())),
        };
        self.thread_state = Some(state);
        result
    }

    /// Waits to the thread to terminate (without request to terminate).
    /// This call will block until a `RunningThread::terminate()` was called.
    /// Join a thread not spawned will not wait.
    pub fn join(&mut self) {
        log::trace!("Waiting to finish thread: [{}]", self.name);
        let state = match self.thread_state.take().unwrap() {
            ThreadState::Ready(state) => state,
            ThreadState::Running(thread, _) => thread.join().expect(OTHER_THREAD_ERR),
            ThreadState::Finishing(thread) => thread.join().expect(OTHER_THREAD_ERR),
        };
        self.thread_state = Some(ThreadState::Ready(state));
        log::trace!("Finished to waiting thread: [{}]", self.name);
    }

    /// Check if the thread is running.
    /// A `RunningThread` is considered running if `RunningThread::spawn()` was called but
    /// `RunningThread::terminate()` not.
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
    pub fn state(mut self) -> Result<S, RunningErr> {
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
        if self.terminate().is_ok() {
            self.join();
        }
    }
}

/// Type used to indicate if the thread is currently running.
/// This type is sharable adn clonable among threads.
pub struct ThreadRunningIndicator {
    running: Arc<AtomicBool>,
}

impl ThreadRunningIndicator {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Clone for ThreadRunningIndicator {
    fn clone(&self) -> Self {
        Self { running: self.running.clone() }
    }
}

#[derive(Debug)]
pub struct RunningErr(String);

impl std::error::Error for RunningErr {}

impl std::fmt::Display for RunningErr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "The action requires that the thread [{}] was not running or be terminated",
            self.0,
        )
    }
}

#[derive(Debug)]
pub struct NotRunningErr;

impl std::error::Error for NotRunningErr {}

impl std::fmt::Display for NotRunningErr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "The action requires that the thread is running")
    }
}
