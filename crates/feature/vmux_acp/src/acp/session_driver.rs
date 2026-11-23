use std::path::PathBuf;

use bevy::prelude::Bundle;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use vmux_api::protocol::{AgentAttachment, ApprovalDecision, ServiceMessage};
use vmux_api::session::RemoteSession;
use vmux_ecs::ProcessId;
use vmux_process::ProcessRuntime;

use super::driver::AcpMcpServers;
use super::{
    AcpSessionAgentInfo, AcpSessionConfigRequest, AcpSessionInbox, AcpSessionReceivers,
    AcpSessionRuntime, AcpSessionStatusRequest, AcpSessionWake, AcpSessions, ApproveAcpTool,
    CancelAcpSession, CloseAcpSession, FindAcpSession, ListAcpSessions, RebindAcpSession,
    SendAcpPrompt, SetAcpConfig, SnapshotAcpSession, SpawnAcpSession, SubscribeAcpSession,
};

impl AcpSessions {
    pub fn new(runtime: Handle, wake: mpsc::UnboundedSender<()>) -> (Self, impl Bundle) {
        let (spawns, spawn_inbox) = mpsc::unbounded_channel();
        let (prompts, prompt_inbox) = mpsc::unbounded_channel();
        let (approvals, approval_inbox) = mpsc::unbounded_channel();
        let (configs, config_inbox) = mpsc::unbounded_channel();
        let (cancels, cancel_inbox) = mpsc::unbounded_channel();
        let (subscriptions, subscription_inbox) = mpsc::unbounded_channel();
        let (snapshots, snapshot_inbox) = mpsc::unbounded_channel();
        let (agent_infos, agent_info_inbox) = mpsc::unbounded_channel();
        let (config_states, config_state_inbox) = mpsc::unbounded_channel();
        let (statuses, status_inbox) = mpsc::unbounded_channel();
        let (lists, list_inbox) = mpsc::unbounded_channel();
        let (lookups, lookup_inbox) = mpsc::unbounded_channel();
        let (rebinds, rebind_inbox) = mpsc::unbounded_channel();
        let (closes, close_inbox) = mpsc::unbounded_channel();
        (
            Self {
                spawns,
                prompts,
                approvals,
                configs,
                cancels,
                subscriptions,
                snapshots,
                agent_infos,
                config_states,
                statuses,
                lists,
                lookups,
                rebinds,
                closes,
                wake: wake.clone(),
            },
            (
                AcpSessionRuntime(runtime),
                AcpSessionWake(wake),
                AcpSessionInbox(AcpSessionReceivers {
                    spawns: spawn_inbox,
                    prompts: prompt_inbox,
                    approvals: approval_inbox,
                    configs: config_inbox,
                    cancels: cancel_inbox,
                    subscriptions: subscription_inbox,
                    snapshots: snapshot_inbox,
                    agent_infos: agent_info_inbox,
                    config_states: config_state_inbox,
                    statuses: status_inbox,
                    lists: list_inbox,
                    lookups: lookup_inbox,
                    rebinds: rebind_inbox,
                    closes: close_inbox,
                }),
            ),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        &self,
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: PathBuf,
        anchor: ProcessId,
        processes: ProcessRuntime,
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        resume: Option<String>,
    ) -> Result<(), String> {
        self.spawn_with_mcp(
            sid,
            agent_id,
            command,
            args,
            env,
            cwd,
            anchor,
            processes,
            mcp_command,
            mcp_args,
            Vec::new(),
            resume,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_with_mcp(
        &self,
        sid: String,
        agent_id: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: PathBuf,
        anchor: ProcessId,
        processes: ProcessRuntime,
        mcp_command: Option<String>,
        mcp_args: Vec<String>,
        managed_mcp_servers: Vec<vmux_api::protocol::ManagedMcpServer>,
        resume: Option<String>,
    ) -> Result<(), String> {
        let mcp_servers =
            AcpMcpServers::from_sources(mcp_command, mcp_args, managed_mcp_servers, cwd.clone());
        let (response, receiver) = oneshot::channel();
        self.spawns
            .send(SpawnAcpSession {
                sid,
                agent_id,
                command,
                args,
                env,
                cwd,
                anchor,
                processes,
                mcp_servers,
                resume,
                response: Some(response),
            })
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        self.wake
            .send(())
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "ACP session spawn was cancelled".to_string())
    }

    pub async fn send_prompt(
        &self,
        sid: String,
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
        preferred_mode: Option<String>,
    ) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .prompts
            .send(SendAcpPrompt {
                sid,
                text,
                context,
                attachments,
                preferred_mode,
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }

    pub async fn approve(&self, sid: String, call_id: String, decision: ApprovalDecision) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .approvals
            .send(ApproveAcpTool {
                sid,
                call_id,
                decision,
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }

    pub async fn set_config(
        &self,
        sid: String,
        request_id: u64,
        config_id: Option<String>,
        value: String,
    ) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .configs
            .send(SetAcpConfig {
                sid,
                request_id,
                config_id,
                value,
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }

    pub async fn cancel(&self, sid: String) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .cancels
            .send(CancelAcpSession {
                sid,
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }

    pub async fn subscribe(&self, sid: String) -> Option<broadcast::Receiver<ServiceMessage>> {
        let (response, receiver) = oneshot::channel();
        self.subscriptions
            .send(SubscribeAcpSession {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub async fn snapshot(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.snapshots
            .send(SnapshotAcpSession {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub async fn agent_info(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.agent_infos
            .send(AcpSessionAgentInfo {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub async fn config_state(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.config_states
            .send(AcpSessionConfigRequest {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub async fn status(&self, sid: String) -> Option<ServiceMessage> {
        let (response, receiver) = oneshot::channel();
        self.statuses
            .send(AcpSessionStatusRequest {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub async fn remote_sessions(&self) -> Vec<RemoteSession> {
        let (response, receiver) = oneshot::channel();
        if self
            .lists
            .send(ListAcpSessions {
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return Vec::new();
        }
        receiver.await.unwrap_or_default()
    }

    pub async fn remote_session(&self, sid: String) -> Option<RemoteSession> {
        let (response, receiver) = oneshot::channel();
        self.lookups
            .send(FindAcpSession {
                sid,
                response: Some(response),
            })
            .ok()?;
        self.wake.send(()).ok()?;
        receiver.await.ok().flatten()
    }

    pub async fn rebind_cwd(&self, sid: String, cwd: PathBuf) -> Result<PathBuf, String> {
        let (response, receiver) = oneshot::channel();
        self.rebinds
            .send(RebindAcpSession {
                sid,
                cwd,
                response: Some(response),
            })
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        self.wake
            .send(())
            .map_err(|_| "ACP session runtime unavailable".to_string())?;
        receiver
            .await
            .map_err(|_| "ACP workspace rebind was cancelled".to_string())?
    }

    pub async fn close(&self, sid: String) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .closes
            .send(CloseAcpSession {
                sid,
                response: Some(response),
            })
            .is_err()
            || self.wake.send(()).is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }
}
