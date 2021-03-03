use std::sync::{atomic::{Ordering, AtomicUsize}};

/// Information about the type of resource
pub enum ResourceType {
    Listener,
    Remote,
}

/// Identifier of the network resource. each network resource has an unique id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ResourceId {
    id: usize,
}

impl ResourceId {
    const RESOURCE_TYPE_BIT: usize = 1 << 63; // 1 bit
    const ADAPTER_ID_MASK: u8 = 0b01111111; // 7 bits
    const ADAPTER_ID_POS: usize = 8 * 7; // 7 bytes
    const ADAPTER_ID_MASK_OVER_ID: usize =
        (Self::ADAPTER_ID_MASK as usize) << Self::ADAPTER_ID_POS; // 7 bytes

    fn new(id: usize, resource_type: ResourceType, adapter_id: u8) -> Self {
        assert_eq!(adapter_id & Self::ADAPTER_ID_MASK, adapter_id,
            "The adapter_id value uses bits outside of the mask");
        Self {
            id: match resource_type {
                ResourceType::Listener => id
                    | Self::RESOURCE_TYPE_BIT
                    | (adapter_id as usize) << Self::ADAPTER_ID_POS,
                ResourceType::Remote => id,
            }
        }
    }

    /// Creates a [ResourceId] from an id
    pub fn from(raw: usize) -> Self {
        Self { id: raw }
    }

    /// Returns the internal representation of this id
    pub fn raw(&self) -> usize {
        self.id
    }

    /// Returns the ResourceType of this id
    pub fn resource_type(&self) -> ResourceType {
        if self.id & Self::RESOURCE_TYPE_BIT != 0 {
            ResourceType::Listener
        }
        else {
            ResourceType::Remote
        }
    }

    /// Returns the associated transport adapter id.
    pub fn adapter_id(&self) -> u8 {
        ((self.id & Self::ADAPTER_ID_MASK_OVER_ID) >> Self::ADAPTER_ID_POS) as u8
    }
}

/// Used by the adapters in order to create unique ids for their resources.
pub struct ResourceIdGenerator {
    last: AtomicUsize,
    adapter_id: u8,
}

impl ResourceIdGenerator {
    pub fn new(adapter_id: u8) -> Self {
        Self { last: AtomicUsize::new(0), adapter_id, }
    }

    /// Generates a new id.
    /// This id will contain information about the [ResourceType] and the associated adapter.
    pub fn generate(&self, resource_type: ResourceType) -> ResourceId {
        let last = self.last.fetch_add(1, Ordering::SeqCst);
        ResourceId::new(last, resource_type, self.adapter_id)
    }
}
