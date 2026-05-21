use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use bevy::tasks::{IoTaskPool, Task};
use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::run_state::ToolDispatchOutput;

#[derive(Clone, Debug)]
pub struct DispatchResult {
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug)]
pub struct EmitDispatch {
    pub request_id: [u8; 16],
    pub target: vmux_mcp::tools::DispatchTarget,
}

pub static EMIT_CHANNEL: LazyLock<(Sender<EmitDispatch>, Receiver<EmitDispatch>)> =
    LazyLock::new(unbounded);

static PENDING: LazyLock<Mutex<HashMap<[u8; 16], Sender<DispatchResult>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register_pending(request_id: [u8; 16]) -> Receiver<DispatchResult> {
    let (tx, rx) = unbounded::<DispatchResult>();
    PENDING.lock().unwrap().insert(request_id, tx);
    rx
}

pub fn deliver(request_id: &[u8; 16], result: DispatchResult) {
    if let Some(tx) = PENDING.lock().unwrap().remove(request_id) {
        let _ = tx.send(result);
    }
}

pub fn spawn_tool_task(
    call_id: String,
    name: String,
    args: serde_json::Value,
) -> Task<ToolDispatchOutput> {
    IoTaskPool::get().spawn(async move {
        let target = match vmux_mcp::tools::dispatch_from_tool_call(&name, args) {
            Ok(t) => t,
            Err(e) => {
                return ToolDispatchOutput {
                    call_id,
                    content: e,
                    is_error: true,
                };
            }
        };

        let request_id = *uuid::Uuid::new_v4().as_bytes();
        let rx = register_pending(request_id);
        let _ = EMIT_CHANNEL.0.send(EmitDispatch { request_id, target });

        let result = match rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(r) => r,
            Err(_) => DispatchResult {
                content: "tool dispatch timed out (60s)".to_string(),
                is_error: true,
            },
        };

        ToolDispatchOutput {
            call_id,
            content: result.content,
            is_error: result.is_error,
        }
    })
}
