use std::sync::{
    atomic::{Ordering, AtomicUsize},
};

/// Information about the type of resource
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ResourceType {
    Listener,
    Remote,
}

/// Unique identifier of a network resource.
/// The identifier wrap 3 values,
/// - The type, that can be a value of [ResourceType].
/// - The adapter id, that represent the adapter that creates this id
/// - The base value: that is an unique identifier of the resource inside of its adapter.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId {
    id: usize,
}

impl ResourceId {
    const RESOURCE_TYPE_BIT: usize = 1 << (Self::ADAPTER_ID_POS + 7); // 1 bit
    const ADAPTER_ID_POS: usize = 8 * 7; // 7 bytes
    const ADAPTER_ID_MASK: u8 = 0b01111111; // 7 bits
    const ADAPTER_ID_MASK_OVER_ID: usize = (Self::ADAPTER_ID_MASK as usize) << Self::ADAPTER_ID_POS;
    const BASE_VALUE_MASK_OVER_ID: usize = 0x0FFFFFFF; // 7 bytes

    pub const ADAPTER_ID_MAX: usize = Self::ADAPTER_ID_MASK as usize + 1; // 128

    fn new(adapter_id: u8, resource_type: ResourceType, base_value: usize) -> Self {
        assert_eq!(
            adapter_id & Self::ADAPTER_ID_MASK,
            adapter_id,
            "The adapter_id value uses bits outside of the mask"
        );
        assert_eq!(
            base_value & Self::BASE_VALUE_MASK_OVER_ID,
            base_value,
            "The base_value value uses bits outside of the mask"
        );

        let resource_type = match resource_type {
            ResourceType::Listener => Self::RESOURCE_TYPE_BIT,
            ResourceType::Remote => 0,
        };

        Self { id: base_value | resource_type | (adapter_id as usize) << Self::ADAPTER_ID_POS }
    }

    /// Creates a [ResourceId] from an id
    pub fn from(raw: usize) -> Self {
        Self { id: raw }
    }

    /// Returns the internal representation of this id
    pub fn raw(&self) -> usize {
        self.id
    }

    /// Returns the [ResourceType] of this resource
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

    /// Returns the unique identifier inside this adapter.
    pub fn base_value(&self) -> usize {
        self.id & Self::BASE_VALUE_MASK_OVER_ID
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let resource_type = match self.resource_type() {
            ResourceType::Listener => "L",
            ResourceType::Remote => "R",
        };
        write!(f, "{}.{}.{}", self.adapter_id(), resource_type, self.base_value())
    }
}

impl std::fmt::Debug for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

/// Used by the adapters in order to create unique ids for their resources.
pub struct ResourceIdGenerator {
    last: AtomicUsize,
    adapter_id: u8,
    resource_type: ResourceType,
}

impl ResourceIdGenerator {
    pub fn new(adapter_id: u8, resource_type: ResourceType) -> Self {
        Self { last: AtomicUsize::new(0), adapter_id, resource_type }
    }

    /// Generates a new id.
    /// This id will contain information about the [ResourceType] and the associated adapter.
    pub fn generate(&self) -> ResourceId {
        let last = self.last.fetch_add(1, Ordering::SeqCst);
        ResourceId::new(self.adapter_id, self.resource_type, last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_value() {
        let low_base_value = 0;

        let resource_id = ResourceId::new(1, ResourceType::Listener, low_base_value);
        assert_eq!(resource_id.base_value(), low_base_value);

        let high_base_value = ResourceId::BASE_VALUE_MASK_OVER_ID;

        let resource_id = ResourceId::new(1, ResourceType::Listener, high_base_value);
        assert_eq!(resource_id.base_value(), high_base_value);
    }

    #[test]
    fn resource_type() {
        let resource_id = ResourceId::new(0, ResourceType::Listener, 0);
        assert_eq!(resource_id.resource_type(), ResourceType::Listener);
        assert_eq!(resource_id.adapter_id(), 0);

        let resource_id = ResourceId::new(0, ResourceType::Remote, 0);
        assert_eq!(resource_id.resource_type(), ResourceType::Remote);
        assert_eq!(resource_id.adapter_id(), 0);
    }

    #[test]
    fn adapter_id() {
        let adapter_id = ResourceId::ADAPTER_ID_MASK;

        let resource_id = ResourceId::new(adapter_id, ResourceType::Listener, 0);
        assert_eq!(resource_id.adapter_id(), adapter_id);
        assert_eq!(resource_id.resource_type(), ResourceType::Listener);

        let resource_id = ResourceId::new(adapter_id, ResourceType::Remote, 0);
        assert_eq!(resource_id.adapter_id(), adapter_id);
        assert_eq!(resource_id.resource_type(), ResourceType::Remote);
    }
}
