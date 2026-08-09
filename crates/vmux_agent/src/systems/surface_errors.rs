use bevy::prelude::*;

use crate::client::acp::AcpSession;
use crate::components::AgentSession;
use crate::run_state::AgentRunState;
use crate::run_state_kind::{AgentRunStateKind, LastRunStateKind};
use crate::toast::{AgentToast, ToastLevel};

/// On a transition into `Errored`, fire a toast. The chat page renders the errored run-state as a
/// styled inline card, so the error is not also pushed into the transcript (which duplicated it).
pub fn surface_errors(
    mut writer: MessageWriter<AgentToast>,
    mut q: Query<(
        &AgentRunState,
        &mut LastRunStateKind,
        Option<&AgentSession>,
        Option<&AcpSession>,
    )>,
) {
    for (state, mut last, page, acp) in &mut q {
        // Resolve the session id from either a Page/CLI session or an ACP session.
        let Some(sid) = page
            .map(|s| s.sid.clone())
            .or_else(|| acp.map(|s| s.sid.clone()))
        else {
            continue;
        };
        let cur = AgentRunStateKind::from(state);
        if last.0 == cur {
            continue;
        }
        last.0 = cur;
        if cur != AgentRunStateKind::Errored {
            continue;
        }
        let AgentRunState::Errored(msg) = state else {
            continue;
        };
        writer.write(AgentToast {
            session_sid: sid,
            level: ToastLevel::Error,
            message: msg.clone(),
        });
    }
}

#[cfg(test)]
#[path = "surface_errors.test.rs"]
mod tests;
