use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::protocol::AgentAttachment;
use vmux_chat::host::ChatView;
use vmux_ecs::{PageOpenTask, PendingPrompt, PendingPromptAttachments};
use vmux_layout::space::FocusedSpace;
use vmux_layout::tab::Tab;
use vmux_session::{AcpSession, AgentConversationTitle, PromptQueue};
use vmux_setting::AppSettings;

use super::acp::registry::RegistryAgent;
use super::attach::AcpAgentAttachment;
use crate::route::AcpRoute;

#[derive(SystemParam)]
pub(super) struct AcpCatalog<'w, 's> {
    agents: Query<'w, 's, &'static RegistryAgent>,
}

#[derive(SystemParam)]
pub(super) struct AgentPageOpenWorkspace<'w, 's> {
    active_space: FocusedSpace<'w, 's>,
    child_of: Query<'w, 's, &'static ChildOf>,
    tabs: Query<'w, 's, &'static Tab>,
    hierarchy: vmux_layout::space::SpaceHierarchy<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct PageOpener<'w, 's> {
    sessions: Query<'w, 's, &'static AcpSession>,
    pub(super) commands: Commands<'w, 's>,
    pub(super) settings: Res<'w, AppSettings>,
    catalog: AcpCatalog<'w, 's>,
}

impl AcpCatalog<'_, '_> {
    pub(super) fn agent(&self, id: &str) -> Option<&RegistryAgent> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    fn installed_id(&self) -> Option<String> {
        self.agents
            .iter()
            .find(|agent| agent.is_installed())
            .map(|agent| agent.id.clone())
    }

    pub(super) fn icon(&self, id: &str) -> Option<String> {
        self.agent(id).and_then(|agent| agent.icon.clone())
    }

    pub(super) fn profile_name(
        &self,
        id: &str,
        config: Option<&vmux_setting::AcpAgentConfig>,
    ) -> String {
        self.agent(id)
            .map(|agent| agent.name.trim())
            .filter(|name| !name.is_empty())
            .or_else(|| {
                let name = config?.name.trim();
                (!name.is_empty()).then_some(name)
            })
            .unwrap_or(id)
            .to_string()
    }
}

impl AgentPageOpenWorkspace<'_, '_> {
    pub(super) fn startup_dir(
        &self,
        entity: Entity,
        settings: &AppSettings,
    ) -> Option<vmux_setting::StartupDir> {
        let space_id = self
            .hierarchy
            .id(entity)
            .or_else(|| self.active_space.id().map(str::to_string))?;
        vmux_setting::StartupDir::resolve(settings, &space_id, None)
    }

    pub(super) fn tab(&self, entity: Entity) -> Option<(Entity, Option<String>)> {
        let mut current = entity;
        loop {
            if let Ok(tab) = self.tabs.get(current) {
                return Some((current, tab.startup_dir.clone()));
            }
            current = self.child_of.get(current).ok()?.parent();
        }
    }
}

impl PageOpener<'_, '_> {
    pub(super) fn apply(
        &mut self,
        task: &PageOpenTask,
        initial_prompt: Option<String>,
        initial_attachments: Vec<AgentAttachment>,
        transition_webview: Option<Entity>,
        default_cwd: &Path,
    ) -> Result<(), String> {
        let target = match AcpRoute::parse(&task.url) {
            Some(AcpRoute::AcpDefault) => {
                let id = self
                    .settings
                    .agent
                    .acp
                    .first()
                    .map(|config| config.id.clone())
                    .or_else(|| self.catalog.installed_id())
                    .ok_or_else(|| "no ACP agent is configured or installed".to_string())?;
                AcpRoute::Acp { id, sid: None }
            }
            Some(target) => target,
            None => return Err(format!("malformed agent URL '{}'", task.url)),
        };
        match target {
            AcpRoute::Acp { id, sid } => {
                let config = self
                    .settings
                    .agent
                    .acp
                    .iter()
                    .find(|config| config.id == id);
                if config.is_none() && self.catalog.agent(&id).is_none() {
                    return Err(format!("ACP agent unavailable for '{id}'"));
                }
                if self
                    .sessions
                    .get(task.stack)
                    .is_ok_and(|session| session.agent_id == id)
                {
                    return Ok(());
                }
                if transition_webview.is_none() {
                    self.commands.entity(task.stack).despawn_children();
                }
                let routing_sid = uuid::Uuid::new_v4().to_string();
                let icon = self.catalog.icon(&id);
                let name = self.catalog.profile_name(&id, config);
                let request = AcpAgentAttachment::new(
                    id,
                    name,
                    routing_sid,
                    default_cwd.to_path_buf(),
                    icon,
                    sid,
                );
                self.commands.entity(task.stack).insert(request);
                if let Some(webview) = transition_webview {
                    self.commands
                        .entity(task.stack)
                        .insert(vmux_ecs::EntityTarget::<ChatView>::new(webview));
                }
                self.insert_prompt(task.stack, initial_prompt, initial_attachments);
                Ok(())
            }
            AcpRoute::AcpDefault => unreachable!(),
        }
    }

    fn insert_prompt(
        &mut self,
        stack: Entity,
        initial_prompt: Option<String>,
        initial_attachments: Vec<AgentAttachment>,
    ) {
        let prompt = initial_prompt.unwrap_or_default();
        if prompt.trim().is_empty() && initial_attachments.is_empty() {
            return;
        }
        if let Some(title) = AgentConversationTitle::from_prompt(&prompt) {
            self.commands.entity(stack).insert(title);
        }
        let mut queue = PromptQueue::default();
        queue.enqueue_with_attachments(prompt, initial_attachments);
        self.commands
            .entity(stack)
            .insert(queue)
            .remove::<(PendingPrompt, PendingPromptAttachments)>();
    }
}
