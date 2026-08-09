//! Serving desktop pages from the phone.
//!
//! A shared page speaks one language: it emits typed payloads under an event id and subscribes to
//! ids it wants pushed back. On the desktop those ids cross a process boundary into Bevy. Here they
//! cross the QUIC link instead, and the page cannot tell.
//!
//! Subscriptions are still polled. Nothing about QUIC requires that — a session transcript is
//! already pushed down a long-lived stream — but no event id other than the team roster has a
//! server-initiated route yet, and the roster moves rarely enough not to have forced one.
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
/// The team roster only moves when an agent starts or finishes, so staleness costs little and a
/// push route has not been worth adding.
const POLL_INTERVAL_MS: u32 = 3_000;

pub struct MobileHost {
    api: Api,
}

/// Route shared pages through `api` for the rest of this app's life.
pub fn install(api: Api) {
    install_host(Rc::new(MobileHost { api }));
}

impl PageHost for MobileHost {
    fn send(&self, _id: &str, _bytes: &[u8]) -> Result<(), EventListenerError> {
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
                    // Pairing is gone, or there is no such session. Neither is fixed by asking
                    // again every few seconds.
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
