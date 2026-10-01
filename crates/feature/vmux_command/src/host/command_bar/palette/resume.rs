use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::chat::{ResumableSessions, ResumeListRequest};
use vmux_api::command_bar::{
    CommandBarUiState, CommandBarUiStatePatch, CommandPaletteDraftRequest,
};
use vmux_ecs::host::UiStateWrite;

use super::super::model::PaletteQuery;

use super::{
    NewPalette, OpenVersion, PaletteDraftInput, PaletteSnapshot, RequestGeneration, project,
};

pub(super) struct PaletteResumePlugin;

impl Plugin for PaletteResumePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(update)
            .add_observer(receive_page)
            .add_systems(PreUpdate, attach)
            .add_systems(PostUpdate, request_more.after(project));
    }
}

#[derive(Component, Default)]
pub(super) struct PaletteResume {
    open: OpenVersion,
    active: bool,
    generation: RequestGeneration,
    inflight: Option<ResumeFlight>,
}

fn attach(pages: Query<Entity, NewPalette<PaletteResume>>, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).insert(PaletteResume::default());
    }
}

fn update(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&mut PaletteResume, &mut PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut resume, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let Some(opened) = resume.open.accept(request.open_id) else {
        return;
    };
    if opened {
        resume.active = false;
        resume.generation.advance();
        snapshot.0.open_id = request.open_id;
        snapshot.0.sessions.clear();
        snapshot.0.sessions_total = 0;
        snapshot.0.sessions_loading = false;
    }
    let wants_resume = request.start && ResumeQuery::matches(&request.query);
    if wants_resume == resume.active {
        return;
    }
    resume.active = wants_resume;
    resume.generation.advance();
    if !wants_resume {
        snapshot.0.sessions_loading = false;
        return;
    }
    snapshot.0.sessions.clear();
    snapshot.0.sessions_total = 0;
    snapshot.0.sessions_loading = true;
    if let Some(request) = resume.request_page(target, 0, &mut snapshot) {
        commands.trigger(request);
    }
}

fn receive_page(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(&mut PaletteResume, &mut PaletteSnapshot)>,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<ResumableSessions>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let target = trigger.event().webview();
    let Ok((mut resume, mut snapshot)) = palettes.get_mut(target) else {
        return;
    };
    resume.apply_page(response, &mut snapshot);
}

fn request_more(
    mut palettes: Query<(
        Entity,
        &PaletteDraftInput,
        &mut PaletteResume,
        &mut PaletteSnapshot,
    )>,
    mut commands: Commands,
) {
    for (target, input, mut resume, mut snapshot) in &mut palettes {
        if let Some(request) = resume.maybe_request_page(input.selected, target, &mut snapshot) {
            commands.trigger(request);
        }
    }
}

impl PaletteResume {
    fn request_page(
        &mut self,
        target: Entity,
        offset: u32,
        snapshot: &mut PaletteSnapshot,
    ) -> Option<UiInput<ResumeListRequest>> {
        if !self.active || self.inflight.is_some() {
            return None;
        }
        snapshot.0.sessions_loading = true;
        let request_id = self.generation.current();
        self.inflight = Some(ResumeFlight {
            open_generation: self.open.generation(),
            generation: self.generation.current(),
            request_id,
            offset,
        });
        Some(UiInput {
            webview: target,
            payload: ResumeListRequest {
                request_id,
                query: String::new(),
                offset,
            },
        })
    }

    fn apply_page(&mut self, response: &ResumableSessions, snapshot: &mut PaletteSnapshot) {
        let Some(flight) = self.inflight.take() else {
            return;
        };
        let accepted = flight.open_generation == self.open.generation()
            && self.generation.matches(flight.generation)
            && self.active
            && flight.request_id == response.request_id
            && response.query.is_empty()
            && flight.offset == response.offset;
        if accepted {
            snapshot.0.sessions_total = response.total;
            snapshot.0.sessions_loading = false;
            if response.offset == 0 {
                snapshot.0.sessions.clone_from(&response.sessions);
            } else {
                snapshot.0.sessions.extend(response.sessions.clone());
            }
        }
    }

    fn maybe_request_page(
        &mut self,
        selected: usize,
        target: Entity,
        snapshot: &mut PaletteSnapshot,
    ) -> Option<UiInput<ResumeListRequest>> {
        if !self.active || self.inflight.is_some() {
            return None;
        }
        let loaded = snapshot.0.sessions.len() as u32;
        if loaded == 0 || loaded >= snapshot.0.sessions_total {
            return None;
        }
        if (selected as u32).saturating_add(10) < loaded {
            return None;
        }
        self.request_page(target, loaded, snapshot)
    }
}

struct ResumeFlight {
    open_generation: u64,
    generation: u64,
    request_id: u64,
    offset: u32,
}

struct ResumeQuery;

impl ResumeQuery {
    fn matches(query: &str) -> bool {
        PaletteQuery::new(query)
            .slash_token()
            .is_some_and(|(name, _)| "resume".starts_with(&name.to_lowercase()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_query_accepts_partial_command_names() {
        assert!(ResumeQuery::matches("/r"));
        assert!(ResumeQuery::matches("/resume"));
        assert!(!ResumeQuery::matches("/upload"));
        assert!(!ResumeQuery::matches("resume"));
    }
}
