use std::sync::{
    atomic::{Ordering, AtomicUsize},
};

/// Information about the type of resource
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ResourceType {
    Local,
    Remote,
}

/// Unique identifier of a network resource in your system.
/// The identifier wrap 3 values,
/// - The type, that can be a value of [ResourceType].
/// - The adapter id, that represents the adapter that creates this id
/// - The base value: that is an unique identifier of the resource inside of its adapter.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId {
    id: usize,
}

impl ResourceId {
    const ADAPTER_ID_POS: usize = 0;
    const RESOURCE_TYPE_POS: usize = 7;
    const BASE_VALUE_POS: usize = 8;

    const ADAPTER_ID_MASK: u8 = 0b01111111; // 5 bits
    const BASE_VALUE_MASK: usize = 0xFFFFFFFFFFFFFF00_u64 as usize; // 7 bytes

    pub const MAX_BASE_VALUE: usize = (Self::BASE_VALUE_MASK >> Self::BASE_VALUE_POS);
    pub const MAX_ADAPTER_ID: u8 = (Self::ADAPTER_ID_MASK >> Self::ADAPTER_ID_POS);
    pub const MAX_ADAPTERS: usize = Self::MAX_ADAPTER_ID as usize + 1;

    fn new(adapter_id: u8, resource_type: ResourceType, base_value: usize) -> Self {
        debug_assert!(
            adapter_id <= Self::MAX_ADAPTER_ID,
            "The adapter_id must be less than {}",
            Self::MAX_ADAPTER_ID + 1,
        );

        debug_assert!(
            base_value <= Self::MAX_BASE_VALUE,
            "The base_value must be less than {}",
            Self::MAX_BASE_VALUE + 1,
        );

        let resource_type = match resource_type {
            ResourceType::Local => 1 << Self::RESOURCE_TYPE_POS,
            ResourceType::Remote => 0,
        };

        Self {
            id: (adapter_id as usize) << Self::ADAPTER_ID_POS
                | resource_type
                | base_value << Self::BASE_VALUE_POS,
        }
    }

    /// Returns the internal representation of this id
    pub fn raw(&self) -> usize {
        self.id
    }

    /// Returns the [ResourceType] of this resource
    pub fn resource_type(&self) -> ResourceType {
        if self.id & (1 << Self::RESOURCE_TYPE_POS) != 0 {
            ResourceType::Local
        }
        else {
            ResourceType::Remote
        }
    }

    /// Tells if the id preresents a local resource.
    pub fn is_local(&self) -> bool {
        self.resource_type() == ResourceType::Local
    }

    /// Tells if the id preresents a remote resource.
    pub fn is_remote(&self) -> bool {
        self.resource_type() == ResourceType::Remote
    }

    /// Returns the associated adapter id.
    /// Note that this returned value is the same as the value of [`crate::network::Transport::id()`]
    /// if that transport uses the same adapter.
    pub fn adapter_id(&self) -> u8 {
        ((self.id & Self::ADAPTER_ID_MASK as usize) >> Self::ADAPTER_ID_POS) as u8
    }

    /// Returns the unique resource identifier inside the associated adapter.
    pub fn base_value(&self) -> usize {
        (self.id & Self::BASE_VALUE_MASK) >> Self::BASE_VALUE_POS
    }
}

impl From<usize> for ResourceId {
    fn from(raw: usize) -> Self {
        Self { id: raw }
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let resource_type = match self.resource_type() {
            ResourceType::Local => "L",
            ResourceType::Remote => "R",
        };
        write!(f, "[{}.{}.{}]", self.adapter_id(), resource_type, self.base_value())
    }
}

impl std::fmt::Debug for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
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
    /// This id will contain information about the [`ResourceType`] and the associated adapter.
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

        let resource_id = ResourceId::new(1, ResourceType::Local, low_base_value);
        assert_eq!(low_base_value, resource_id.base_value());

        let high_base_value = ResourceId::MAX_BASE_VALUE;

        let resource_id = ResourceId::new(1, ResourceType::Local, high_base_value);
        assert_eq!(high_base_value, resource_id.base_value());
    }

    #[test]
    fn resource_type() {
        let resource_id = ResourceId::new(0, ResourceType::Local, 0);
        assert_eq!(ResourceType::Local, resource_id.resource_type());
        assert_eq!(0, resource_id.adapter_id());

        let resource_id = ResourceId::new(0, ResourceType::Remote, 0);
        assert_eq!(ResourceType::Remote, resource_id.resource_type());
        assert_eq!(0, resource_id.adapter_id());
    }

    #[test]
    fn adapter_id() {
        let adapter_id = ResourceId::MAX_ADAPTER_ID;

        let resource_id = ResourceId::new(adapter_id, ResourceType::Local, 0);
        assert_eq!(adapter_id, resource_id.adapter_id());
        assert_eq!(ResourceType::Local, resource_id.resource_type());

        let resource_id = ResourceId::new(adapter_id, ResourceType::Remote, 0);
        assert_eq!(adapter_id, resource_id.adapter_id());
        assert_eq!(ResourceType::Remote, resource_id.resource_type());
    }
}
