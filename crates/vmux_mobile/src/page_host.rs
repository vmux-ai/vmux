//! Serving desktop pages from the phone.
//!
//! A shared page speaks one language: it emits typed payloads under an event id and subscribes to
//! ids it wants pushed back. On the desktop those ids cross a process boundary into Bevy. Here the
//! desktop is reachable only over HTTP, so a subscription becomes a poll and the JSON that comes
//! back is re-encoded as the rkyv the page already knows how to decode. The page cannot tell.
//!
//! Ids with no route are refused rather than silently accepted, so a half-served page reports as
//! much instead of rendering empty and looking broken.

use std::rc::Rc;

use dioxus::prelude::*;
use vmux_chat::platform::sleep_ms;
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::{BytesListener, PageHost, install_host};
use vmux_wire::team::{TEAM_EVENT, TeamEvent, TeamMemberRow};

use crate::Api;

/// How often a subscription re-reads the desktop.
///
/// The desktop pushes on change; HTTP cannot, so this trades staleness for a request. The team
/// roster only moves when an agent starts or finishes, so it does not need to be quick.
const POLL_INTERVAL_MS: u32 = 3_000;

pub struct MobileHost {
    api: Api,
}

/// Route shared pages through `api` for the rest of this app's life.
pub fn install(api: Api) {
    install_host(Rc::new(MobileHost { api }));
}

impl PageHost for MobileHost {
    fn emit(&self, _id: &str, _bytes: &[u8]) -> Result<(), EventListenerError> {
        Err(EventListenerError::Unsupported)
    }

    fn listen(&self, id: &str, mut on_bytes: BytesListener) -> Result<(), EventListenerError> {
        if id != TEAM_EVENT {
            return Err(EventListenerError::Unsupported);
        }
        let api = self.api.clone();
        // Scope-bound, so it stops when the page that subscribed goes away.
        spawn(async move {
            let mut last: Option<Vec<TeamMemberRow>> = None;
            loop {
                match api.team().await {
                    Ok(members) => {
                        if last.as_ref() != Some(&members) {
                            let payload = TeamEvent {
                                members: members.clone(),
                            };
                            if let Ok(bytes) = rkyv::to_bytes::<rkyv::rancor::Error>(&payload) {
                                on_bytes(&bytes);
                            }
                            last = Some(members);
                        }
                    }
                    // Pairing is gone, or the route is not there — a relay too old to carry it
                    // answers the same way. Neither is fixed by asking again every few seconds.
                    Err(crate::ApiError::Unauthorized | crate::ApiError::NotFound) => return,
                    // Anything else is likely the network, which does heal.
                    Err(crate::ApiError::Message(_)) => {}
                }
                sleep_ms(POLL_INTERVAL_MS).await;
            }
        });
        Ok(())
    }
}
