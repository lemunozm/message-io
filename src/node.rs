use crate::network::{self, NetworkController, NetworkProcessor, NetEvent, Endpoint, ResourceId};
use crate::events::{self, EventSender, EventReceiver};
use crate::util::thread::{NamespacedThread, OTHER_THREAD_ERR};

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration};
use std::collections::{VecDeque};

/// Event returned by the node when some network or signal is received.
pub enum NodeEvent<'a, S> {
    /// The event comes from the network.
    /// See [`NetEvent`] to know about the different network events.
    Network(NetEvent<'a>),

    /// The event comes from an user signal.
    /// See [`EventSender`] to know about how to send signals.
    Signal(S),
}

impl<'a, S> NodeEvent<'a, S> {
    /// Assume the event is a `NodeEvent::Network`, panics if not.
    pub fn network(self) -> NetEvent<'a> {
        match self {
            NodeEvent::Network(net_event) => net_event,
            NodeEvent::Signal(..) => panic!("NodeEvent must be a NetEvent"),
        }
    }

    /// Assume the event is a `NodeEvent::Signal`, panics if not.
    pub fn signal(self) -> S {
        match self {
            NodeEvent::Network(..) => panic!("NodeEvent must be a Signal"),
            NodeEvent::Signal(signal) => signal,
        }
    }
}

/// Creates a node.
/// This function offers two instances: a [`NodeHandler`] to perform network and signals actions
/// and a [`NodeListener`] to receive the events the node receives.
///
/// Note that [`NodeListener`] is already listen for events from its creation.
/// In order to get the listened events you can call [`NodeListener::for_each()`]
pub fn split<S: Send>() -> (NodeHandler<S>, NodeListener<S>) {
    let (network_controller, network_processor) = network::split();
    let (signal_sender, signal_receiver) = events::split();
    let running = Arc::new(AtomicBool::new(true));

    let node_handler = NodeHandler::new(network_controller, signal_sender, running.clone());
    let node_listener = NodeListener::new(network_processor, signal_receiver, running);

    (node_handler, node_listener)
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
    /// See [`EventSender`].
    pub fn signals(&self) -> &EventSender<S> {
        &self.signals
    }

    /// Finalizes the node processing.
    /// No more events would be processing after this call.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Check if the node is running.
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
    const SAMPLING_TIMEOUT: u64 = 50; //ms

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
            NamespacedThread::new("node-network-cache-thread", move || {
                let timeout = Some(Duration::from_millis(Self::SAMPLING_TIMEOUT));
                while cache_running.load(Ordering::Relaxed) {
                    network_processor.process_poll_event(timeout, &mut |net_event| {
                        log::trace!("Cached {:?}", net_event);
                        cache.push_back(net_event.into());
                    });
                }
                (network_processor, cache)
            })
        };

        NodeListener { network_cache_thread, cache_running, signal_receiver, running }
    }

    /// Run the receiver asynchronously in order to dispatch events into the `event_callback`.
    /// A `NodeTask` representing this job will be returned.
    /// Destroying this object will result in blocking the current thread while
    /// [`NodeHandler::stop`] not be performed.
    ///
    /// In order to allow the node working asynchronously, you can save the `NodeTask` in
    /// an object with a longer lifetime.
    ///
    /// # Examples
    /// Synchronous node:
    /// ```
    /// fn main() {
    ///     let (node, listener) = split();
    ///     node.signals().send_with_timer((), Duration::from_secs(1));
    ///     node.network().listen(Transport::FramedTcp, "0.0.0.0:1234");
    ///
    ///     listener.for_each(move |event| {
    ///         match event {
    ///              NodeEvent::Network(net_event) => { /* Your logic here */ },
    ///              NodeEvent::Signal(_) => node.stop());
    ///         }
    ///     });
    ///     //Blocked here until signal is received (1 sec).
    ///     println!("Node is stopped");
    /// }
    /// ```
    ///
    /// Asynchronous node:
    /// ```
    /// fn main() {
    ///     let (node, listener) = split();
    ///     node.signals().send_with_timer((), Duration::from_secs(1));
    ///     node.network().listen(Transport::FramedTcp, "0.0.0.0:1234");
    ///
    ///     let task = listener.for_each(move |event| {
    ///         match event {
    ///              NodeEvent::Network(net_event) => { /* Your logic here */ },
    ///              NodeEvent::Signal(_) => node.stop());
    ///         }
    ///     });
    ///
    ///     // ...
    ///     println!("Node is running");
    ///     // ...
    ///
    ///     drop(task); //Blocked here until signal is received (1 sec).
    ///     println!("Node is stopped");
    /// }
    /// ```
    pub fn for_each(
        mut self,
        event_callback: impl FnMut(NodeEvent<S>) + Send + 'static,
    ) -> NodeTask {
        // Stop cache events
        self.cache_running.store(false, Ordering::Relaxed);
        let (mut network_processor, mut cache) = self.network_cache_thread.join();

        let multiplexed_callback = Arc::new(Mutex::new(event_callback));

        // To avoid processing stops while the node is configuring,
        // the user callback locked until the function ends.
        let _locked = multiplexed_callback.lock().expect(OTHER_THREAD_ERR);

        let network_thread = {
            let callback = multiplexed_callback.clone();
            let running = self.running.clone();

            NamespacedThread::new("node-network-thread", move || {
                // Dispatch the catched events first.
                while let Some(event) = cache.pop_front() {
                    let mut event_callback = callback.lock().expect(OTHER_THREAD_ERR);
                    let net_event = event.borrow();
                    log::trace!("Read from cache {:?}", net_event);
                    event_callback(NodeEvent::Network(net_event));
                    if !running.load(Ordering::Relaxed) {
                        return
                    }
                }

                let timeout = Some(Duration::from_millis(Self::SAMPLING_TIMEOUT));
                while running.load(Ordering::Relaxed) {
                    network_processor.process_poll_event(timeout, &mut |net_event| {
                        let mut event_callback = callback.lock().expect(OTHER_THREAD_ERR);
                        if running.load(Ordering::Relaxed) {
                            event_callback(NodeEvent::Network(net_event));
                        }
                    });
                }
            })
        };

        let signal_thread = {
            let callback = multiplexed_callback.clone();
            let mut signal_receiver = std::mem::take(&mut self.signal_receiver);
            let running = self.running.clone();

            NamespacedThread::new("node-signal-thread", move || {
                let timeout = Duration::from_millis(Self::SAMPLING_TIMEOUT);
                while running.load(Ordering::Relaxed) {
                    if let Some(signal) = signal_receiver.receive_timeout(timeout) {
                        let mut event_callback = callback.lock().expect(OTHER_THREAD_ERR);
                        if running.load(Ordering::Relaxed) {
                            event_callback(NodeEvent::Signal(signal));
                        }
                    }
                }
            })
        };

        NodeTask { _network_thread: network_thread, _signal_thread: signal_thread }
    }
}

impl<S: Send + 'static> Drop for NodeListener<S> {
    fn drop(&mut self) {
        self.cache_running.store(false, Ordering::Relaxed);
    }
}

/// Entity used to ensure the lifetime of `NodeListener::for_each()` call.
/// The node will process events asynchronously while this entity lives.
pub struct NodeTask {
    _network_thread: NamespacedThread<()>,
    _signal_thread: NamespacedThread<()>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration};

    #[test]
    fn create_node_and_drop() {
        let (node, _listener) = split::<()>();
        assert!(node.is_running());
        // listener dropped here.
    }

    #[test]
    fn sync_node() {
        let (node, listener) = split();
        assert!(node.is_running());
        node.signals().send_with_timer((), Duration::from_millis(1000));

        let inner_node = node.clone();
        listener.for_each(move |_| inner_node.stop());

        // Since here `NodeTask` is already dropped just after listener call,
        // the node is considered not running.
        assert!(!node.is_running());
    }

    #[test]
    fn async_node() {
        let (node, listener) = split();
        assert!(node.is_running());
        node.signals().send_with_timer("Check", Duration::from_millis(250));

        let checked = Arc::new(AtomicBool::new(false));
        let inner_checked = checked.clone();
        let inner_node = node.clone();
        let _node_task = listener.for_each(move |event| match event.signal() {
            "Stop" => inner_node.stop(),
            "Check" => inner_checked.store(true, Ordering::Relaxed),
            _ => unreachable!(),
        });

        // Since here `NodeTask` is living, the node is considered running.
        assert!(node.is_running());
        std::thread::sleep(Duration::from_millis(500));
        assert!(checked.load(Ordering::Relaxed));
        assert!(node.is_running());
        node.signals().send("Stop");
    }
}
