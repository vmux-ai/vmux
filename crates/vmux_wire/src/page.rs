use bevy_ecs::message::Message;

#[derive(Message)]
pub struct PageEmit {
    pub id: &'static str,
    pub bytes: Vec<u8>,
}

impl PageEmit {
    pub fn of<T>(id: &'static str, payload: &T) -> Option<Self>
    where
        T: for<'a> rkyv::Serialize<
                rkyv::api::high::HighSerializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'a>,
                    rkyv::rancor::Error,
                >,
            >,
    {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(payload).ok()?;
        Some(Self {
            id,
            bytes: bytes.to_vec(),
        })
    }
}
