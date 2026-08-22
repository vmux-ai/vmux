//! Handing a built payload to whichever page asked for it.
//!
//! The world-side half of page transport. `vmux_ui::hooks::transport` is the other end, where a
//! host turns an id into a listener; this is what a plugin writes when the thing that page draws
//! has changed. Neither end knows the other exists — the id is the whole contract, which is what
//! lets the same page crate be served by Bevy on the desktop and by a QUIC link on a phone.
//!
//! Here rather than in an app, because the id belongs to the page crate that defines it: a crate
//! that answers a URL should be able to say what it emits without an app relaying it.

use bevy_ecs::message::Message;

/// A payload for whichever page registered for `id`.
#[derive(Message)]
pub struct PageEmit {
    pub id: &'static str,
    pub bytes: Vec<u8>,
}

impl PageEmit {
    /// Serialise `payload` for delivery under `id`.
    ///
    /// `None` when it will not serialise, which a caller should treat as "say nothing this turn"
    /// rather than as an error: a page holding its last good payload reads better than one handed
    /// a broken frame, and the next change will produce another.
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
