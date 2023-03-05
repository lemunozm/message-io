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

/// Event returned by [`NodeListener::for_each()`] and [`NodeListener::for_each_async()`]
/// when some network event or signal is received.
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

impl<'a, S: std::fmt::Debug> std::fmt::Debug for NodeEvent<'a, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeEvent::Network(net_event) => write!(f, "NodeEvent::Network({net_event:?})"),
            NodeEvent::Signal(signal) => write!(f, "NodeEvent::Signal({signal:?})"),
        }
    }
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

/// Analogous to [`NodeEvent`] but without reference the data.
/// This kind of event is dispatched by `NodeListener::to_event_queue()`.
/// It is useful when you need to move an [`NodeEvent`]
#[derive(Clone)]
pub enum StoredNodeEvent<S> {
    /// The `StoredNodeEvent` is an event that comes from the network.
    /// See [`NetEvent`] to know about the different network events.
    Network(StoredNetEvent),

    /// The `StoredNodeEvent` is a signal.
    /// A signal is an event produced by the own node to itself.
    /// You can send signals with timers or priority.
    /// See [`EventSender`] to know about how to send signals.
    Signal(S),
}

impl<S: std::fmt::Debug> std::fmt::Debug for StoredNodeEvent<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoredNodeEvent::Network(net_event) => write!(f, "NodeEvent::Network({net_event:?})"),
            StoredNodeEvent::Signal(signal) => write!(f, "NodeEvent::Signal({signal:?})"),
        }
    }
}

impl<S> StoredNodeEvent<S> {
    /// Assume the event is a [`StoredNodeEvent::Network`], panics if not.
    pub fn network(self) -> StoredNetEvent {
        match self {
            StoredNodeEvent::Network(net_event) => net_event,
            StoredNodeEvent::Signal(..) => panic!("NodeEvent must be a NetEvent"),
        }
    }

    /// Assume the event is a [`StoredNodeEvent::Signal`], panics if not.
    pub fn signal(self) -> S {
        match self {
            StoredNodeEvent::Network(..) => panic!("NodeEvent must be a Signal"),
            StoredNodeEvent::Signal(signal) => signal,
        }
    }
}

impl<S> From<NodeEvent<'_, S>> for StoredNodeEvent<S> {
    fn from(node_event: NodeEvent<'_, S>) -> Self {
        match node_event {
            NodeEvent::Network(net_event) => StoredNodeEvent::Network(net_event.into()),
            NodeEvent::Signal(signal) => StoredNodeEvent::Signal(signal),
        }
    }
}

/// Analogous to [`NetEvent`] but with static lifetime (without reference the data).
/// This kind of event is dispatched by `NodeListener::to_event_queue()`
/// and can be easily stored in any container.
#[derive(Debug, Clone)]
pub enum StoredNetEvent {
    Connected(Endpoint, bool),
    Accepted(Endpoint, ResourceId),
    Message(Endpoint, Vec<u8>),
    Disconnected(Endpoint),
}

impl From<NetEvent<'_>> for StoredNetEvent {
    fn from(net_event: NetEvent<'_>) -> Self {
        match net_event {
            NetEvent::Connected(endpoint, status) => Self::Connected(endpoint, status),
            NetEvent::Accepted(endpoint, id) => Self::Accepted(endpoint, id),
            NetEvent::Message(endpoint, data) => Self::Message(endpoint, Vec::from(data)),
            NetEvent::Disconnected(endpoint) => Self::Disconnected(endpoint),
        }
    }
}

impl StoredNetEvent {
    /// Use this `StoredNetEvent` as a `NetEvent` referencing its data.
    pub fn borrow(&self) -> NetEvent<'_> {
        match self {
            Self::Connected(endpoint, status) => NetEvent::Connected(*endpoint, *status),
            Self::Accepted(endpoint, id) => NetEvent::Accepted(*endpoint, *id),
            Self::Message(endpoint, data) => NetEvent::Message(*endpoint, data),
            Self::Disconnected(endpoint) => NetEvent::Disconnected(*endpoint),
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
    let running = AtomicBool::new(true);

    let handler = NodeHandler(Arc::new(NodeHandlerImpl {
        network: network_controller,
        signals: signal_sender,
        running,
    }));

    let listener = NodeListener::new(network_processor, signal_receiver, handler.clone());

    (handler, listener)
}

struct NodeHandlerImpl<S> {
    network: NetworkController,
    signals: EventSender<S>,
    running: AtomicBool,
}

/// A shareable and clonable entity that allows to deal with
/// the network, send signals and stop the node.
pub struct NodeHandler<S>(Arc<NodeHandlerImpl<S>>);

impl<S> NodeHandler<S> {
    /// Returns a reference to the NetworkController to deal with the network.
    /// See [`NetworkController`]
    pub fn network(&self) -> &NetworkController {
        &self.0.network
    }

    /// Returns a reference to the EventSender to send signals to the node.
    /// Signals are events that the node send to itself useful in situation where you need
    /// to "wake up" the [`NodeListener`] to perform some action.
    /// See [`EventSender`].
    pub fn signals(&self) -> &EventSender<S> {
        &self.0.signals
    }

    /// Finalizes the [`NodeListener`].
    /// After this call, no more events will be processed by [`NodeListener::for_each()`].
    pub fn stop(&self) {
        self.0.running.store(false, Ordering::Relaxed);
    }

    /// Check if the node is running.
    /// Note that the node is running and listening events from its creation,
    /// not only once you call to [`NodeListener::for_each()`].
    pub fn is_running(&self) -> bool {
        self.0.running.load(Ordering::Relaxed)
    }
}

impl<S: Send + 'static> Clone for NodeHandler<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Listen events for network and signal events.
pub struct NodeListener<S: Send + 'static> {
    network_cache_thread: NamespacedThread<(NetworkProcessor, VecDeque<StoredNetEvent>)>,
    cache_running: Arc<AtomicBool>,
    signal_receiver: EventReceiver<S>,
    handler: NodeHandler<S>,
}

impl<S: Send + 'static> NodeListener<S> {
    fn new(
        mut network_processor: NetworkProcessor,
        signal_receiver: EventReceiver<S>,
        handler: NodeHandler<S>,
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

        NodeListener { network_cache_thread, cache_running, signal_receiver, handler }
    }

    /// Iterate indefinitely over all generated `NetEvent`.
    /// This function will work until [`NodeHandler::stop()`] is called.
    ///
    /// Note that any events generated before calling this function (e.g. some connection was done)
    /// will be stored and offered once you call `for_each()`.
    /// # Example
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::Transport;
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    /// let (id, addr) = handler.network().listen(Transport::FramedTcp, "127.0.0.1:0").unwrap();
    ///
    /// listener.for_each(move |event| match event {
    ///     NodeEvent::Network(net_event) => { /* Your logic here */ },
    ///     NodeEvent::Signal(_) => handler.stop(),
    /// });
    /// // Blocked here until handler.stop() is called (1 sec).
    /// println!("Node is stopped");
    /// ```
    pub fn for_each(mut self, mut event_callback: impl FnMut(NodeEvent<S>)) {
        // Stop cache events
        self.cache_running.store(false, Ordering::Relaxed);
        let (mut network_processor, mut cache) = self.network_cache_thread.join();

        // Dispatch the catched events first.
        while let Some(event) = cache.pop_front() {
            let net_event = event.borrow();
            log::trace!("Read from cache {:?}", net_event);
            event_callback(NodeEvent::Network(net_event));
            if !self.handler.is_running() {
                return
            }
        }

        crossbeam_utils::thread::scope(|scope| {
            let multiplexed = Arc::new(Mutex::new(event_callback));

            let _signal_thread = {
                let mut signal_receiver = std::mem::take(&mut self.signal_receiver);
                let handler = self.handler.clone();

                // This struct is used to allow passing the no sendable event_callback
                // into the signal thread.
                // It is safe because the thread are scoped and the callback is managed by a lock,
                // so only one call is performed at the same time.
                // It implies that any object moved into the callback do not have
                // any concurrence issues.
                #[allow(clippy::type_complexity)]
                struct SendableEventCallback<'a, S>(Arc<Mutex<dyn FnMut(NodeEvent<S>) + 'a>>);
                #[allow(clippy::non_send_fields_in_send_ty)]
                unsafe impl<'a, S> Send for SendableEventCallback<'a, S> {}

                let multiplexed = SendableEventCallback(multiplexed.clone());

                scope
                    .builder()
                    .name(String::from("node-network-thread"))
                    .spawn(move |_| {
                        while handler.is_running() {
                            if let Some(signal) = signal_receiver.receive_timeout(*SAMPLING_TIMEOUT)
                            {
                                let mut event_callback =
                                    multiplexed.0.lock().expect(OTHER_THREAD_ERR);
                                if handler.is_running() {
                                    event_callback(NodeEvent::Signal(signal));
                                }
                            }
                        }
                    })
                    .unwrap()
            };

            while self.handler.is_running() {
                network_processor.process_poll_event(Some(*SAMPLING_TIMEOUT), |net_event| {
                    let mut event_callback = multiplexed.lock().expect(OTHER_THREAD_ERR);
                    if self.handler.is_running() {
                        event_callback(NodeEvent::Network(net_event));
                    }
                });
            }
        })
        .unwrap();
    }

    /// Similar to [`NodeListener::for_each()`] but it returns the control to the user
    /// after calling it. The events will be processed asynchronously.
    /// A `NodeTask` representing this asynchronous job is returned.
    /// Destroying this object will result in blocking the current thread until
    /// [`NodeHandler::stop()`] is called.
    ///
    /// In order to allow the node working asynchronously, you can move the `NodeTask` to a
    /// an object with a longer lifetime.
    ///
    /// # Example
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::Transport;
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    /// let (id, addr) = handler.network().listen(Transport::FramedTcp, "127.0.0.1:0").unwrap();
    ///
    /// let task = listener.for_each_async(move |event| match event {
    ///      NodeEvent::Network(net_event) => { /* Your logic here */ },
    ///      NodeEvent::Signal(_) => handler.stop(),
    /// });
    /// // for_each_async() will act asynchronous during 'task' lifetime.
    ///
    /// // ...
    /// println!("Node is running");
    /// // ...
    ///
    /// drop(task); // Blocked here until handler.stop() is called (1 sec).
    /// // Also task.wait(); can be called doing the same (but taking a mutable reference).
    ///
    /// println!("Node is stopped");
    /// ```
    pub fn for_each_async(
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
            let handler = self.handler.clone();

            NamespacedThread::spawn("node-network-thread", move || {
                while let Some(event) = cache.pop_front() {
                    let net_event = event.borrow();
                    log::trace!("Read from cache {:?}", net_event);
                    let mut event_callback = multiplexed.lock().expect(OTHER_THREAD_ERR);
                    event_callback(NodeEvent::Network(net_event));
                    if !handler.is_running() {
                        return
                    }
                }

                while handler.is_running() {
                    network_processor.process_poll_event(Some(*SAMPLING_TIMEOUT), |net_event| {
                        let mut event_callback = multiplexed.lock().expect(OTHER_THREAD_ERR);
                        if handler.is_running() {
                            event_callback(NodeEvent::Network(net_event));
                        }
                    });
                }
            })
        };

        let signal_thread = {
            let multiplexed = multiplexed.clone();
            let mut signal_receiver = std::mem::take(&mut self.signal_receiver);
            let handler = self.handler.clone();

            NamespacedThread::spawn("node-signal-thread", move || {
                while handler.is_running() {
                    if let Some(signal) = signal_receiver.receive_timeout(*SAMPLING_TIMEOUT) {
                        let mut event_callback = multiplexed.lock().expect(OTHER_THREAD_ERR);
                        if handler.is_running() {
                            event_callback(NodeEvent::Signal(signal));
                        }
                    }
                }
            })
        };

        NodeTask { network_thread, signal_thread }
    }

    /// Consumes the listener to create a `NodeTask` and an `EventReceiver` where the events
    /// of this node will be sent.
    /// The events will be sent to the `EventReceiver` during the `NodeTask` lifetime.
    /// The aim of this method is to offer a synchronous way of working with a *node*,
    /// without using a clousure.
    /// This easier API management has a performance cost.
    /// Compared to [`NodeListener::for_each()`], this function adds latency because the
    /// node event must be copied and no longer reference data from the internal socket buffer.
    ///
    /// # Example
    /// ```
    /// use message_io::node::{self, StoredNodeEvent as NodeEvent};
    /// use message_io::network::Transport;
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    /// let (id, addr) = handler.network().listen(Transport::FramedTcp, "127.0.0.1:0").unwrap();
    ///
    /// let (task, mut receiver) = listener.enqueue();
    ///
    /// loop {
    ///     match receiver.receive() {
    ///         NodeEvent::Network(net_event) => { /* Your logic here */ },
    ///         NodeEvent::Signal(_) => break handler.stop(),
    ///     }
    /// }
    /// ```
    pub fn enqueue(self) -> (NodeTask, EventReceiver<StoredNodeEvent<S>>) {
        let (sender, receiver) = events::split::<StoredNodeEvent<S>>();
        let task = self.for_each_async(move |node_event| sender.send(node_event.into()));
        (task, receiver)
    }
}

impl<S: Send + 'static> Drop for NodeListener<S> {
    fn drop(&mut self) {
        self.cache_running.store(false, Ordering::Relaxed);
    }
}

/// Entity used to ensure the lifetime of [`NodeListener::for_each_async()`] call.
/// The node will process events asynchronously while this entity lives.
/// The destruction of this entity will block until the task is finished.
/// If you want to "unblock" the thread that drops this entity call to
/// [`NodeHandler::stop()`] before or from another thread.
#[must_use = "The NodeTask must be used or the asynchronous task will be dropped in return"]
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
        let _node_task = listener.for_each_async(move |event| match event.signal() {
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
    fn enqueue() {
        let (handler, listener) = split();
        assert!(handler.is_running());
        handler.signals().send_with_timer((), Duration::from_millis(1000));

        let (mut task, mut receiver) = listener.enqueue();
        assert!(handler.is_running());

        receiver.receive_timeout(Duration::from_millis(2000)).unwrap().signal();
        handler.stop();

        assert!(!handler.is_running());
        task.wait();
    }

    #[test]
    fn wait_task() {
        let (handler, listener) = split();

        handler.signals().send_with_timer((), Duration::from_millis(1000));

        let inner_handler = handler.clone();
        listener.for_each_async(move |_| inner_handler.stop()).wait();

        assert!(!handler.is_running());
    }

    #[test]
    fn wait_already_waited_task() {
        let (handler, listener) = split();

        handler.signals().send_with_timer((), Duration::from_millis(1000));

        let inner_handler = handler.clone();
        let mut task = listener.for_each_async(move |_| inner_handler.stop());
        assert!(handler.is_running());
        task.wait();
        assert!(!handler.is_running());
        task.wait();
        assert!(!handler.is_running());
    }
}
