use crate::network::{self, NetworkController, NetworkProcessor, NetEvent, Endpoint, ResourceId};
use crate::events::{self, EventSender, EventReceiver};
use crate::util::thread::{NamespacedThread, OTHER_THREAD_ERR};

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration};
use std::collections::{VecDeque};

lazy_static::lazy_static! {
    static ref SAMPLING_TIMEOUT: Duration = Duration::from_millis(50);
}

/// Event returned by [`NodeListener::for_each()`] when some network or signal is received.
pub enum NodeEvent<'a, S> {
    /// The `NodeEvent` is an event that comes from the network.
    /// See [`NetEvent`] to know about the different network events.
    Network(NetEvent<'a>),

    /// The `NodeEvent` is a signal.
    /// A signal is an event produced by the own node to itself.
    /// You can send signals with timers or priority.
    /// See [`EventSender`] to know about how to send signals.
    Signal(S),
}

impl<'a, S> NodeEvent<'a, S> {
    /// Assume the event is a [`NodeEvent::Network`], panics if not.
    pub fn network(self) -> NetEvent<'a> {
        match self {
            NodeEvent::Network(net_event) => net_event,
            NodeEvent::Signal(..) => panic!("NodeEvent must be a NetEvent"),
        }
    }

    /// Assume the event is a [`NodeEvent::Signal`], panics if not.
    pub fn signal(self) -> S {
        match self {
            NodeEvent::Network(..) => panic!("NodeEvent must be a Signal"),
            NodeEvent::Signal(signal) => signal,
        }
    }
}

/// Creates a node already working.
/// This function offers two instances: a [`NodeHandler`] to perform network and signals actions
/// and a [`NodeListener`] to receive the events the node receives.
///
/// Note that [`NodeListener`] is already listen for events from its creation.
/// In order to get the listened events you can call [`NodeListener::for_each()`]
/// Any event happened before `for_each()` call will be also dispatched.
///
/// # Examples
/// ```rust
/// use message_io::node::{self, NodeEvent};
///
/// enum Signal {
///     Close,
///     Tick,
///     //Other signals here.
/// }
///
/// let (handler, listener) = node::split();
///
/// handler.signals().send_with_timer(Signal::Close, std::time::Duration::from_secs(1));
///
/// listener.for_each(move |event| match event {
///     NodeEvent::Network(_) => { /* ... */ },
///     NodeEvent::Signal(signal) => match signal {
///         Signal::Close => handler.stop(), //Received after 1 sec
///         Signal::Tick => { /* ... */ },
///     },
/// });
/// ```
///
/// In case you don't use signals, specify the node type with an unit (`()`) type.
/// ```
/// use message_io::node::{self};
///
/// let (handler, listener) = node::split::<()>();
/// ```
pub fn split<S: Send>() -> (NodeHandler<S>, NodeListener<S>) {
    let (network_controller, network_processor) = network::split();
    let (signal_sender, signal_receiver) = events::split();
    let running = Arc::new(AtomicBool::new(true));

    let handler = NodeHandler::new(network_controller, signal_sender, running.clone());
    let listener = NodeListener::new(network_processor, signal_receiver, running);

    (handler, listener)
}

/// A shareable and clonable entity that allows to deal with
/// the network, send signals and stop the node.
pub struct NodeHandler<S> {
    network: Arc<NetworkController>,
    signals: EventSender<S>,
    running: Arc<AtomicBool>,
}

impl<S> NodeHandler<S> {
    fn new(network: NetworkController, signals: EventSender<S>, running: Arc<AtomicBool>) -> Self {
        Self { network: Arc::new(network), signals, running }
    }

    /// Returns a reference to the NetworkController to deal with the network.
    /// See [`NetworkController`]
    pub fn network(&self) -> &NetworkController {
        &self.network
    }

    /// Returns a reference to the EventSender to send signals to the node.
    /// Signals are events that the node send to itself useful in situation where you need
    /// to "wake up" the [`NodeListener`] to perform some action.
    /// See [`EventSender`].
    pub fn signals(&self) -> &EventSender<S> {
        &self.signals
    }

    /// Finalizes the [`NodeListener`].
    /// After this call, no more events will be processed by [`NodeListener::for_each()`].
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Check if the node is running.
    /// Note that the node is running and listening events from its creation,
    /// not only once you call to [`NodeListener::for_each()`].
    /// Calling this function only will offer the event to the user to be processed.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl<S: Send + 'static> Clone for NodeHandler<S> {
    fn clone(&self) -> Self {
        Self {
            network: self.network.clone(),
            signals: self.signals.clone(),
            running: self.running.clone(),
        }
    }
}

#[derive(Debug)]
enum StoredNetEvent {
    Connected(Endpoint, ResourceId),
    Message(Endpoint, Vec<u8>),
    Disconnected(Endpoint),
}

impl From<NetEvent<'_>> for StoredNetEvent {
    fn from(net_event: NetEvent<'_>) -> Self {
        match net_event {
            NetEvent::Connected(endpoint, id) => Self::Connected(endpoint, id),
            NetEvent::Message(endpoint, data) => Self::Message(endpoint, Vec::from(data)),
            NetEvent::Disconnected(endpoint) => Self::Disconnected(endpoint),
        }
    }
}

impl StoredNetEvent {
    fn borrow(&self) -> NetEvent<'_> {
        match self {
            Self::Connected(endpoint, id) => NetEvent::Connected(*endpoint, *id),
            Self::Message(endpoint, data) => NetEvent::Message(*endpoint, &data),
            Self::Disconnected(endpoint) => NetEvent::Disconnected(*endpoint),
        }
    }
}

/// Main entity to manipulates the network and signal events easily.
/// The node run asynchronously.
pub struct NodeListener<S: Send + 'static> {
    network_cache_thread: NamespacedThread<(NetworkProcessor, VecDeque<StoredNetEvent>)>,
    cache_running: Arc<AtomicBool>,
    signal_receiver: EventReceiver<S>,
    running: Arc<AtomicBool>,
}

impl<S: Send + 'static> NodeListener<S> {
    fn new(
        mut network_processor: NetworkProcessor,
        signal_receiver: EventReceiver<S>,
        running: Arc<AtomicBool>,
    ) -> NodeListener<S> {
        // Spawn the network thread to be able to perform correctly any network action before
        // for_each() call. Any generated event would be cached and offered to the user when they
        // call for_each().
        let cache_running = Arc::new(AtomicBool::new(true));
        let network_cache_thread = {
            let cache_running = cache_running.clone();
            let mut cache = VecDeque::new();
            NamespacedThread::spawn("node-network-cache-thread", move || {
                while cache_running.load(Ordering::Relaxed) {
                    network_processor.process_poll_event(Some(*SAMPLING_TIMEOUT), |net_event| {
                        log::trace!("Cached {:?}", net_event);
                        cache.push_back(net_event.into());
                    });
                }
                (network_processor, cache)
            })
        };

        NodeListener { network_cache_thread, cache_running, signal_receiver, running }
    }

    /// Iterate indefinitely over all generated `NetEvent`.
    /// This function will work until [`NodeHandler::stop`] was called.
    /// A `NodeTask` representing the asynchronous job is returned.
    /// Destroying this object will result in blocking the current thread until
    /// [`NodeHandler::stop`] was called.
    ///
    /// In order to allow the node working asynchronously, you can move the `NodeTask` to a
    /// an object with a longer lifetime.
    ///
    /// # Examples
    /// **Synchronous** usage:
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::Transport;
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    /// handler.network().listen(Transport::FramedTcp, "0.0.0.0:1234");
    ///
    /// listener.for_each(move |event| match event {
    ///     NodeEvent::Network(net_event) => { /* Your logic here */ },
    ///     NodeEvent::Signal(_) => handler.stop(),
    /// });
    /// // Blocked here until handler.stop() was called (1 sec) because the returned value
    /// // of for_each() is not used (it is dropped just after called the method).
    /// println!("Node is stopped");
    /// ```
    ///
    /// **Asynchronous** usage:
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::Transport;
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    /// handler.network().listen(Transport::FramedTcp, "0.0.0.0:1234");
    ///
    /// let task = listener.for_each(move |event| match event {
    ///      NodeEvent::Network(net_event) => { /* Your logic here */ },
    ///      NodeEvent::Signal(_) => handler.stop(),
    /// });
    /// // for_each() will act asynchronous during 'task' lifetime.
    ///
    /// // ...
    /// println!("Node is running");
    /// // ...
    ///
    /// drop(task); // Blocked here until handler.stop() was called (1 sec).
    /// //also task.wait(); can be called doing the same (but taking a mutable reference).
    ///
    /// println!("Node is stopped");
    /// ```
    /// Note that any events generated before calling this function will be storage
    /// and offered once you call `for_each()`.
    pub fn for_each(
        mut self,
        event_callback: impl FnMut(NodeEvent<S>) + Send + 'static,
    ) -> NodeTask {
        // Stop cache events
        self.cache_running.store(false, Ordering::Relaxed);
        let (mut network_processor, mut cache) = self.network_cache_thread.join();

        let multiplexed = Arc::new(Mutex::new(event_callback));

        // To avoid processing stops while the node is configuring,
        // the user callback locked until the function ends.
        let _locked = multiplexed.lock().expect(OTHER_THREAD_ERR);

        let network_thread = {
            let multiplexed = multiplexed.clone();
            let running = self.running.clone();

            NamespacedThread::spawn("node-network-thread", move || {
                // Dispatch the catched events first.
                while let Some(event) = cache.pop_front() {
                    let mut event_callback = multiplexed.lock().expect(OTHER_THREAD_ERR);
                    let net_event = event.borrow();
                    log::trace!("Read from cache {:?}", net_event);
                    event_callback(NodeEvent::Network(net_event));
                    if !running.load(Ordering::Relaxed) {
                        return
                    }
                }

                while running.load(Ordering::Relaxed) {
                    network_processor.process_poll_event(Some(*SAMPLING_TIMEOUT), |net_event| {
                        let mut event_callback = multiplexed.lock().expect(OTHER_THREAD_ERR);
                        if running.load(Ordering::Relaxed) {
                            event_callback(NodeEvent::Network(net_event));
                        }
                    });
                }
            })
        };

        let signal_thread = {
            let multiplexed = multiplexed.clone();
            let mut signal_receiver = std::mem::take(&mut self.signal_receiver);
            let running = self.running.clone();

            NamespacedThread::spawn("node-signal-thread", move || {
                while running.load(Ordering::Relaxed) {
                    if let Some(signal) = signal_receiver.receive_timeout(*SAMPLING_TIMEOUT) {
                        let mut event_callback = multiplexed.lock().expect(OTHER_THREAD_ERR);
                        if running.load(Ordering::Relaxed) {
                            event_callback(NodeEvent::Signal(signal));
                        }
                    }
                }
            })
        };

        NodeTask { network_thread, signal_thread }
    }
}

impl<S: Send + 'static> Drop for NodeListener<S> {
    fn drop(&mut self) {
        self.cache_running.store(false, Ordering::Relaxed);
    }
}

/// Entity used to ensure the lifetime of [`NodeListener::for_each()`] call.
/// The node will process events asynchronously while this entity lives.
/// The destruction of this entity will block until the task is finished.
/// If you want to "unblock" the thread that drops this entity call to:
/// [`NodeHandler::stop()`]
pub struct NodeTask {
    network_thread: NamespacedThread<()>,
    signal_thread: NamespacedThread<()>,
}

impl NodeTask {
    /// Block the current thread until the task finalizes.
    /// Similar to call `drop(node_task)` but more verbose and without take the ownership.
    /// To finalize the task call [`NodeHandler::stop()`].
    /// Calling `wait()` over an already finished task do not block.
    pub fn wait(&mut self) {
        self.network_thread.try_join();
        self.signal_thread.try_join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration};

    #[test]
    fn create_node_and_drop() {
        let (handler, _listener) = split::<()>();
        assert!(handler.is_running());
        // listener dropped here.
    }

    #[test]
    fn sync_node() {
        let (handler, listener) = split();
        assert!(handler.is_running());
        handler.signals().send_with_timer((), Duration::from_millis(1000));

        let inner_handler = handler.clone();
        listener.for_each(move |_| inner_handler.stop());

        // Since here `NodeTask` is already dropped just after listener call,
        // the node is considered not running.
        assert!(!handler.is_running());
    }

    #[test]
    fn async_node() {
        let (handler, listener) = split();
        assert!(handler.is_running());
        handler.signals().send_with_timer("check", Duration::from_millis(250));

        let checked = Arc::new(AtomicBool::new(false));
        let inner_checked = checked.clone();
        let inner_handler = handler.clone();
        let _node_task = listener.for_each(move |event| match event.signal() {
            "stop" => inner_handler.stop(),
            "check" => inner_checked.store(true, Ordering::Relaxed),
            _ => unreachable!(),
        });

        // Since here `NodeTask` is living, the node is considered running.
        assert!(handler.is_running());
        std::thread::sleep(Duration::from_millis(500));
        assert!(checked.load(Ordering::Relaxed));
        assert!(handler.is_running());
        handler.signals().send("stop");
    }

    #[test]
    fn wait_task() {
        let (handler, listener) = split();

        handler.signals().send_with_timer((), Duration::from_millis(1000));

        let inner_handler = handler.clone();
        listener.for_each(move |_| inner_handler.stop()).wait();

        assert!(!handler.is_running());
    }

    #[test]
    fn wait_already_waited_task() {
        let (handler, listener) = split();

        handler.signals().send_with_timer((), Duration::from_millis(1000));

        let inner_handler = handler.clone();
        let mut task = listener.for_each(move |_| inner_handler.stop());
        assert!(handler.is_running());
        task.wait();
        assert!(!handler.is_running());
        task.wait();
        assert!(!handler.is_running());
    }
}
