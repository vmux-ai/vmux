use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::PageIcon;
use vmux_api::protocol::AgentAttachment;
use vmux_ecs::{
    Cwd, EntityTarget, PageMetadata, PageOpenTask, PendingPrompt, PendingPromptAttachments,
    ProcessAnchor,
};
use vmux_layout::space::FocusedSpace;
use vmux_layout::tab::{StartupDir, Tab, TabWorkspace};
use vmux_session::host::{SessionManagerView, SessionView};
use vmux_session::{AgentConversationTitle, AgentRef, Id, PromptQueue, Route, Session};
use vmux_setting::AppSettings;
use vmux_ui::i18n::translate;

use super::acp::registry::RegistryAgent;
use super::attach::AcpAgentAttachment;
use crate::session::AcpSessionLinks;

#[derive(SystemParam)]
pub(super) struct AcpCatalog<'w, 's> {
    agents: Query<'w, 's, &'static RegistryAgent>,
}

#[derive(SystemParam)]
pub(super) struct AgentPageOpenWorkspace<'w, 's> {
    active_space: FocusedSpace<'w, 's>,
    hierarchy: vmux_layout::space::SpaceHierarchy<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct PageOpener<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            Entity,
            &'static Id<Session>,
            &'static Name,
            &'static Cwd,
            Option<&'static AgentRef>,
            Option<&'static ProcessAnchor>,
        ),
        With<Session>,
    >,
    links: AcpSessionLinks<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    child_of: Query<'w, 's, &'static ChildOf>,
    tabs: Query<'w, 's, &'static mut StartupDir, With<Tab>>,
    workspaces: Query<'w, 's, &'static mut TabWorkspace>,
    queues: Query<'w, 's, &'static mut PromptQueue>,
    transcripts: vmux_session::Transcripts<'w, 's>,
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
}

impl PageOpener<'_, '_> {
    pub(super) fn tab(&self, entity: Entity) -> Option<(Entity, Option<std::path::PathBuf>)> {
        let mut current = entity;
        loop {
            if let Ok(startup_dir) = self.tabs.get(current) {
                return Some((current, startup_dir.0.clone()));
            }
            current = self.child_of.get(current).ok()?.parent();
        }
    }

    pub(super) fn apply(
        &mut self,
        task: &PageOpenTask,
        initial_prompt: Option<String>,
        initial_attachments: Vec<AgentAttachment>,
        transition_webview: Option<Entity>,
        default_cwd: &Path,
    ) -> Result<(), String> {
        let target = match Route::parse(&task.url) {
            Some(target) => target,
            None => return Err(format!("malformed agent URL '{}'", task.url)),
        };
        match target {
            Route::Manager => {
                let url = Route::requested_agent(&task.url)
                    .map(|agent| Route::manager_for_agent(&agent))
                    .unwrap_or_else(|| Route::Manager.url());
                self.open_manager(task.stack, transition_webview, &url);
                Ok(())
            }
            Route::Session(id) => {
                let mut selected = None;
                for (entity, candidate, name, cwd, agent, active) in &self.sessions {
                    if candidate != &id {
                        continue;
                    }
                    selected = Some((
                        entity,
                        name.as_str().to_string(),
                        cwd.0.clone(),
                        agent.map(|agent| agent.as_str().to_string()),
                        self.links
                            .get(entity)
                            .map(|link| link.id().as_str().to_string()),
                        active.map(|active| active.0),
                    ));
                    break;
                }
                let Some((
                    session_entity,
                    session_name,
                    stored_cwd,
                    selected_agent,
                    resume,
                    active,
                )) = selected
                else {
                    return Err(format!("Session '{id}' was not found"));
                };
                let agent_id = selected_agent
                    .or_else(|| {
                        self.settings
                            .agent
                            .acp
                            .first()
                            .map(|config| config.id.clone())
                    })
                    .or_else(|| self.catalog.installed_id())
                    .ok_or_else(|| "no ACP agent is configured or installed".to_string())?;
                let config = self
                    .settings
                    .agent
                    .acp
                    .iter()
                    .find(|config| config.id == agent_id);
                if config.is_none() && self.catalog.agent(&agent_id).is_none() {
                    return Err(format!("ACP agent unavailable for '{agent_id}'"));
                }
                let icon = self.catalog.icon(&agent_id);
                let agent_name = self.catalog.profile_name(&agent_id, config);
                let cwd = if stored_cwd.as_os_str().is_empty() {
                    self.commands
                        .entity(session_entity)
                        .insert(Cwd(default_cwd.to_path_buf()));
                    default_cwd.to_path_buf()
                } else {
                    stored_cwd
                };
                self.bind_workspace(task.stack, &cwd);
                self.commands.entity(task.stack).insert((
                    EntityTarget::<Session>::new(session_entity),
                    PageMetadata {
                        url: Route::Session(id.clone()).url(),
                        title: session_name.clone(),
                        bg_color: None,
                        icon: icon.clone().map(PageIcon::favicon).unwrap_or_default(),
                    },
                ));
                if resume.is_some() && self.transcripts.get(session_entity).messages.is_empty() {
                    self.commands
                        .entity(session_entity)
                        .insert(vmux_session::TranscriptLoading);
                }
                self.replace_children(task.stack, transition_webview);
                self.enqueue_initial_prompt(
                    session_entity,
                    task.stack,
                    initial_prompt,
                    initial_attachments,
                );
                if let Some(anchor) = active {
                    self.open_session_view(
                        task.stack,
                        session_entity,
                        &session_name,
                        &agent_id,
                        anchor,
                        transition_webview,
                    );
                    return Ok(());
                }
                let request = AcpAgentAttachment::new(
                    session_entity,
                    task.stack,
                    agent_id.clone(),
                    &session_name,
                    agent_name,
                    id.as_str().to_string(),
                    cwd,
                    icon,
                    resume,
                );
                self.commands
                    .entity(session_entity)
                    .insert(AgentRef::new(agent_id));
                let operation = self.commands.spawn(request).id();
                if let Some(webview) = transition_webview {
                    self.commands
                        .entity(operation)
                        .insert(EntityTarget::<SessionView>::new(webview));
                }
                Ok(())
            }
        }
    }

    fn open_manager(&mut self, stack: Entity, transition_webview: Option<Entity>, url: &str) {
        let title = translate("sessions-title");
        self.replace_children(stack, transition_webview);
        self.commands
            .entity(stack)
            .remove::<EntityTarget<Session>>()
            .remove::<vmux_command::CommandBarWorkDirectory>()
            .insert(PageMetadata {
                url: url.to_string(),
                title: title.clone(),
                bg_color: None,
                icon: default(),
            });
        let view = transition_webview.unwrap_or_else(|| {
            self.commands
                .spawn((
                    vmux_layout::Browser::hosted_page(url, &title),
                    SessionManagerView,
                    ChildOf(stack),
                ))
                .id()
        });
        self.commands.entity(view).remove::<SessionView>().insert((
            PageMetadata {
                url: url.to_string(),
                title,
                bg_color: None,
                icon: default(),
            },
            SessionManagerView,
        ));
    }

    fn open_session_view(
        &mut self,
        stack: Entity,
        session: Entity,
        name: &str,
        agent_id: &str,
        anchor: vmux_ecs::ProcessId,
        transition_webview: Option<Entity>,
    ) {
        let url = self
            .sessions
            .get(session)
            .map(|(_, id, ..)| Route::Session(id.clone()).url())
            .unwrap_or_else(|_| Route::Manager.url());
        let view = transition_webview.unwrap_or_else(|| {
            self.commands
                .spawn((
                    vmux_layout::Browser::hosted_page(&url, name),
                    SessionView,
                    ChildOf(stack),
                    anchor,
                ))
                .id()
        });
        self.commands.entity(view).insert((
            PageMetadata {
                url,
                title: name.to_string(),
                bg_color: None,
                icon: self
                    .catalog
                    .icon(agent_id)
                    .map(PageIcon::favicon)
                    .unwrap_or_default(),
            },
            SessionView,
            anchor,
        ));
        self.commands.entity(view).remove::<SessionManagerView>();
        self.commands
            .entity(stack)
            .insert(EntityTarget::<Session>::new(session));
    }

    fn replace_children(&mut self, stack: Entity, keep: Option<Entity>) {
        let Ok(children) = self.children.get(stack) else {
            return;
        };
        for child in children.iter() {
            if Some(child) != keep {
                self.commands.entity(child).despawn();
            }
        }
    }

    pub(super) fn bind_workspace(&mut self, stack: Entity, cwd: &Path) {
        let Some((tab_entity, _)) = self.tab(stack) else {
            return;
        };
        if let Ok(mut startup_dir) = self.tabs.get_mut(tab_entity) {
            startup_dir.0 = Some(cwd.to_path_buf());
        }
        let cwd = cwd.to_string_lossy().into_owned();
        if let Ok(mut workspace) = self.workspaces.get_mut(tab_entity) {
            workspace.project_dir.clone_from(&cwd);
        } else {
            self.commands
                .entity(tab_entity)
                .insert(TabWorkspace { project_dir: cwd });
        }
    }

    fn enqueue_initial_prompt(
        &mut self,
        session: Entity,
        stack: Entity,
        initial_prompt: Option<String>,
        initial_attachments: Vec<AgentAttachment>,
    ) {
        let prompt = initial_prompt.unwrap_or_default();
        if prompt.trim().is_empty() && initial_attachments.is_empty() {
            return;
        }
        if let Some(title) = AgentConversationTitle::from_prompt(&prompt) {
            self.commands
                .entity(session)
                .insert((title, vmux_session::TitlePending));
        }
        if let Ok(mut queue) = self.queues.get_mut(session) {
            queue.enqueue_with_attachments(prompt, initial_attachments);
        } else {
            let mut queue = PromptQueue::default();
            queue.enqueue_with_attachments(prompt, initial_attachments);
            self.commands.entity(session).insert(queue);
        }
        self.commands
            .entity(stack)
            .remove::<(PendingPrompt, PendingPromptAttachments)>();
    }
}
