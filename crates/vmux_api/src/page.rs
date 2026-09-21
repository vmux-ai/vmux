use bevy_ecs::message::Message;

#[derive(Message)]
pub struct PageEmit {
    pub id: String,
    pub bytes: Vec<u8>,
}

impl PageEmit {
    pub fn from_event<T>(payload: &T) -> Option<Self>
    where
        T: crate::HostEvent
            + for<'a> rkyv::Serialize<
                rkyv::api::high::HighSerializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'a>,
                    rkyv::rancor::Error,
                >,
            >,
    {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(payload).ok()?;
        Some(Self {
            id: T::id().to_string(),
            bytes: bytes.to_vec(),
        })
    }
}
