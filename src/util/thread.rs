use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

pub const OTHER_THREAD_ERR: &str = "This error is shown because other thread has panicked \
                                  You can safety skip this error.";

pub enum ThreadState<S: Send + 'static> {
    Ready(S),
    Running(JoinHandle<S>, Arc<AtomicBool>),
}

impl<S: Send + 'static> ThreadState<S> {
    fn init_thread(
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

    pub fn run(self, name: &str, callback: impl Fn(&mut S) + Send + 'static) -> ThreadState<S> {
        match self {
            ThreadState::Ready(state) => {
                let thread_running = Arc::new(AtomicBool::new(true));
                let running = thread_running.clone();
                let thread = Self::init_thread(name, running, state, callback);
                ThreadState::Running(thread, thread_running)
            }
            ThreadState::Running(..) => panic!("Thread [{}] already running", name),
        }
    }

    pub fn stop(&self) {
        match self {
            ThreadState::Ready(..) => panic!("Thread is not running"),
            ThreadState::Running(_, running) => {
                running.store(false, Ordering::Relaxed);
            }
        }
    }

    pub fn join(self) -> ThreadState<S> {
        match self {
            ThreadState::Ready(..) => panic!("Thread is not running"),
            ThreadState::Running(thread, _) => {
                let state = thread.join().expect(OTHER_THREAD_ERR);
                ThreadState::Ready(state)
            }
        }
    }
}

pub struct RunnableThread<S: Send + 'static> {
    name: String,
    thread_state: Mutex<Option<ThreadState<S>>>,
}

impl<S: Send> RunnableThread<S> {
    pub fn new(name: &str, state: S) -> Self {
        Self { name: name.into(), thread_state: Mutex::new(Some(ThreadState::Ready(state))) }
    }

    pub fn is_running(&self) -> bool {
        let thread_state = self.thread_state.lock().unwrap();
        match thread_state.as_ref().unwrap() {
            ThreadState::Ready(..) => false,
            ThreadState::Running(..) => true,
        }
    }

    pub fn run(&self, callback: impl Fn(&mut S) + Send + 'static) {
        let mut thread_state = self.thread_state.lock().unwrap();
        *thread_state = Some(thread_state.take().unwrap().run(&self.name, callback));
        log::trace!("Thread [{}] running", self.name);
    }

    pub fn stop(&self) {
        self.thread_state.lock().unwrap().as_ref().unwrap().stop();
        log::trace!("Thread [{}] stopped", self.name);
    }

    pub fn join(&self) {
        log::trace!("Waiting to finish thread: [{}]", self.name);
        let mut thread_state = self.thread_state.lock().unwrap();
        *thread_state = Some(thread_state.take().unwrap().join());
    }
}

impl<S: Send> Drop for RunnableThread<S> {
    fn drop(&mut self) {
        if self.is_running() {
            self.stop();
        }
    }
}
