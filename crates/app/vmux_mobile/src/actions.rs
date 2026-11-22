use std::time::{Duration, Instant};

use bevy_app::{App, Plugin as BevyPlugin, PreUpdate};
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::message::{Message, MessageReader, MessageWriter};
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query};
use vmux_api::prompt_media::{ChatAttachment, InlineMediaQuery};
use vmux_api::protocol::ApprovalDecision;
use vmux_api::session::{AgentAttachment, RemoteEvent, RemoteMediaEntry, RemoteStatus};
use vmux_ecs::{Operation, OperationRetained, OperationState};
use vmux_session::event::ChatComposerEffect;
use vmux_session::host::{
    Attach, Attachments, Browsed, ChatRuntime, Conversation, Models, PublishComposerEffect,
    RemoveAttachment, Reported, Submitted,
};
use vmux_team::roster::Members;

use crate::connection::{
    ActiveConnectionHandle, AuthStage, ClientOperationSequence, ConnectionSource, MobileRoot,
    SourceEpoch, SourceInvalidated, SourceUnauthorized,
};
use crate::io::{
    ApproveCallJob, ApproveCallReceipt, CancelChatJob, CancelChatReceipt, ReadMediaJob,
    ReadMediaReceipt, ReadModelsJob, ReadModelsReceipt, ReadTeamJob, ReadTeamReceipt,
    SelectRemoteModelJob, SelectRemoteModelReceipt, SendPromptJob, SendPromptReceipt,
    SetRemoteEffortJob, SetRemoteEffortReceipt,
};
use crate::remote::ApiError;
use crate::runtime::MountId;
use crate::session::{SelectedSession, SessionGeneration};
use crate::{AdmitSet, DecideSet, InvalidateSet, MaterializeSet};

pub(crate) struct MobileActionsPlugin;

impl BevyPlugin for MobileActionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SubmitPromptIntent>()
            .add_message::<ChangeDraftIntent>()
            .add_message::<RemoveAttachmentIntent>()
            .add_message::<CancelChatIntent>()
            .add_message::<EscapeChatIntent>()
            .add_message::<ApproveCallIntent>()
            .add_message::<SelectModelIntent>()
            .add_message::<SetEffortIntent>()
            .add_message::<AttachPathsIntent>()
            .add_message::<MountInterestAdded>()
            .add_message::<MountRetired>()
            .add_systems(
                PreUpdate,
                (
                    track_mounts,
                    admit_draft,
                    admit_attach,
                    admit_remove_attachment,
                    admit_submit,
                    admit_cancel,
                    admit_escape,
                    admit_approve,
                    admit_select_model,
                    admit_set_effort,
                )
                    .chain()
                    .in_set(AdmitSet),
            )
            .add_systems(PreUpdate, reset_for_session.in_set(InvalidateSet))
            .add_systems(
                PreUpdate,
                (
                    adopt_prompt_send,
                    adopt_simple_actions,
                    adopt_models,
                    adopt_media,
                    adopt_team,
                )
                    .in_set(MaterializeSet),
            )
            .add_systems(
                PreUpdate,
                (poll_models, poll_media, poll_team).in_set(DecideSet),
            );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MountInterestKind {
    Chat,
    Team,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
struct MountInterest {
    mount: MountId,
    kind: MountInterestKind,
}

#[derive(Message)]
pub(crate) struct MountInterestAdded {
    pub(crate) mount: MountId,
    pub(crate) kind: MountInterestKind,
}

#[derive(Message)]
pub(crate) struct MountRetired(pub(crate) MountId);

#[derive(Message)]
pub(crate) struct SubmitPromptIntent {
    pub(crate) text: String,
}

#[derive(Message)]
pub(crate) struct ChangeDraftIntent {
    pub(crate) text: String,
}

#[derive(Message)]
pub(crate) struct RemoveAttachmentIntent {
    pub(crate) path: String,
}

#[derive(Message)]
pub(crate) struct CancelChatIntent;

#[derive(Message)]
pub(crate) struct EscapeChatIntent;

#[derive(Message)]
pub(crate) struct ApproveCallIntent {
    pub(crate) call_id: String,
    pub(crate) decision: ApprovalDecision,
}

#[derive(Message)]
pub(crate) struct SelectModelIntent {
    pub(crate) model_id: String,
}

#[derive(Message)]
pub(crate) struct SetEffortIntent {
    pub(crate) level: String,
}

#[derive(Message)]
pub(crate) struct AttachPathsIntent {
    pub(crate) paths: Vec<String>,
}

#[derive(Component, Default)]
pub(crate) struct ComposerDraft(pub(crate) String);

#[derive(Component, Default)]
pub(crate) struct ComposerEffectRevision(pub(crate) u64);

impl ComposerEffectRevision {
    fn clear_effect(&mut self, draft: &mut ComposerDraft) -> Option<ChatComposerEffect> {
        if draft.0.is_empty() {
            return None;
        }
        self.0 = self.0.wrapping_add(1).max(1);
        draft.0.clear();
        Some(ChatComposerEffect {
            revision: self.0,
            draft: String::new(),
            focus: true,
        })
    }
}

#[derive(Component, Default)]
pub(crate) struct MediaQueryState {
    pub(crate) request_id: u64,
    pub(crate) query: String,
    pub(crate) session_generation: u64,
    pub(crate) dirty: bool,
}

#[derive(Component)]
pub(crate) struct ModelPollState {
    pub(crate) session_generation: u64,
    pub(crate) attempts_remaining: u8,
    pub(crate) next_at: Instant,
    pub(crate) in_flight: bool,
    pub(crate) completed: bool,
}

impl ModelPollState {
    const FETCH_ATTEMPTS: u8 = 5;
    const RETRY_INTERVAL: Duration = Duration::from_secs(1);
}

impl Default for ModelPollState {
    fn default() -> Self {
        Self {
            session_generation: 0,
            attempts_remaining: Self::FETCH_ATTEMPTS,
            next_at: Instant::now(),
            in_flight: false,
            completed: false,
        }
    }
}

#[derive(Component)]
pub(crate) struct TeamPollState {
    pub(crate) next_at: Instant,
    pub(crate) in_flight: bool,
    pub(crate) stopped_for_epoch: Option<u64>,
}

impl TeamPollState {
    const POLL_INTERVAL: Duration = Duration::from_secs(3);
}

impl Default for TeamPollState {
    fn default() -> Self {
        Self {
            next_at: Instant::now(),
            in_flight: false,
            stopped_for_epoch: None,
        }
    }
}

#[derive(Component)]
struct MediaOfferItem;

#[derive(Component)]
struct MediaOfferGeneration(u64);

#[derive(Component)]
struct MediaOfferPath(String);

#[derive(Component)]
struct MediaOfferDetails {
    name: String,
    mime_type: String,
    size: u64,
    is_dir: bool,
    preview_data_url: String,
}

#[derive(Component)]
struct PromptSendOp {
    source_epoch: u64,
    session_generation: u64,
}

#[derive(Component)]
struct ScopedActionOp;

#[derive(Component)]
struct ModelsReadOp {
    source: Entity,
    source_epoch: u64,
    session_generation: u64,
}

#[derive(Component)]
struct MediaReadOp {
    source_epoch: u64,
    session_generation: u64,
    request_id: u64,
    query: String,
}

#[derive(Component)]
struct TeamReadOp {
    source: Entity,
    source_epoch: u64,
}

fn track_mounts(
    mut added: MessageReader<MountInterestAdded>,
    mut retired: MessageReader<MountRetired>,
    roots: Query<Entity, With<MobileRoot>>,
    interests: Query<(Entity, &MountInterest)>,
    mut commands: Commands,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    let mut removed_mounts = Vec::new();
    for MountRetired(mount) in retired.read() {
        removed_mounts.push(*mount);
    }
    if !removed_mounts.is_empty() {
        for (entity, interest) in &interests {
            if removed_mounts.contains(&interest.mount) {
                commands.entity(entity).despawn();
            }
        }
    }
    for event in added.read() {
        if removed_mounts.contains(&event.mount) {
            continue;
        }
        let exists = interests
            .iter()
            .any(|(_, interest)| interest.mount == event.mount && interest.kind == event.kind);
        if !exists {
            commands.spawn((
                ChildOf(root),
                MountInterest {
                    mount: event.mount,
                    kind: event.kind,
                },
            ));
        }
    }
}

#[allow(clippy::type_complexity)]
fn reset_for_session(
    mut invalidated: MessageReader<SourceInvalidated>,
    mut roots: Query<
        (
            &SessionGeneration,
            Option<&SelectedSession>,
            &mut ComposerDraft,
            &mut MediaQueryState,
            &mut ModelPollState,
            &mut TeamPollState,
        ),
        With<MobileRoot>,
    >,
    offers: Query<(Entity, &MediaOfferGeneration), With<MediaOfferItem>>,
    mut commands: Commands,
) {
    let source_reset = invalidated.read().count() > 0;
    let Ok((generation, selected, mut draft, mut media, mut model_poll, mut team_poll)) =
        roots.single_mut()
    else {
        return;
    };
    if source_reset {
        team_poll.stopped_for_epoch = None;
        team_poll.next_at = Instant::now();
    }
    if model_poll.session_generation != generation.0 {
        model_poll.session_generation = generation.0;
        model_poll.attempts_remaining = ModelPollState::FETCH_ATTEMPTS;
        model_poll.next_at = Instant::now();
        model_poll.in_flight = false;
        model_poll.completed = false;
    }
    if media.session_generation != generation.0 {
        media.session_generation = generation.0;
        media.query.clear();
        media.dirty = false;
        if selected.is_none() {
            draft.0.clear();
        }
        for (entity, offer_gen) in &offers {
            if offer_gen.0 != generation.0 {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn admit_draft(
    mut intents: MessageReader<ChangeDraftIntent>,
    mut roots: Query<
        (&SessionGeneration, &mut ComposerDraft, &mut MediaQueryState),
        With<MobileRoot>,
    >,
    offers: Query<Entity, With<MediaOfferItem>>,
    mut browsed: MessageWriter<Browsed>,
    mut commands: Commands,
) {
    let Ok((generation, mut draft, mut media)) = roots.single_mut() else {
        return;
    };
    for intent in intents.read() {
        draft.0.clone_from(&intent.text);
        let query = InlineMediaQuery::parse(&intent.text)
            .map(|parsed| parsed.query.to_string())
            .unwrap_or_default();
        if media.query == query && media.session_generation == generation.0 && media.request_id > 0
        {
            continue;
        }
        media.request_id = media.request_id.wrapping_add(1).max(1);
        media.query = query.clone();
        media.session_generation = generation.0;
        if query.is_empty() {
            media.dirty = false;
            for entity in &offers {
                commands.entity(entity).despawn();
            }
            browsed.write(Browsed {
                request_id: media.request_id,
                query,
                entries: Vec::new(),
            });
        } else {
            media.dirty = true;
        }
    }
}

fn admit_attach(
    mut intents: MessageReader<AttachPathsIntent>,
    roots: Query<&SessionGeneration, With<MobileRoot>>,
    offers: Query<
        (&MediaOfferGeneration, &MediaOfferPath, &MediaOfferDetails),
        With<MediaOfferItem>,
    >,
    mut attach: MessageWriter<Attach>,
) {
    let Ok(generation) = roots.single() else {
        return;
    };
    for intent in intents.read() {
        let mut resolved = Vec::with_capacity(intent.paths.len());
        for path in &intent.paths {
            for (offer_gen, offer_path, details) in &offers {
                if offer_gen.0 != generation.0 || &offer_path.0 != path || details.is_dir {
                    continue;
                }
                resolved.push(ChatAttachment {
                    path: offer_path.0.clone(),
                    name: details.name.clone(),
                    mime_type: details.mime_type.clone(),
                    size: details.size,
                    preview_data_url: details.preview_data_url.clone(),
                    media_url: String::new(),
                });
                break;
            }
        }
        attach.write(Attach(resolved));
    }
}

fn admit_remove_attachment(
    mut intents: MessageReader<RemoveAttachmentIntent>,
    mut remove: MessageWriter<RemoveAttachment>,
) {
    for intent in intents.read() {
        remove.write(RemoveAttachment(intent.path.clone()));
    }
}

type SubmitRootQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static SessionGeneration,
        Option<&'static SelectedSession>,
        &'static mut ClientOperationSequence,
        &'static mut ComposerDraft,
        &'static mut ComposerEffectRevision,
    ),
    With<MobileRoot>,
>;

fn admit_submit(
    mut intents: MessageReader<SubmitPromptIntent>,
    mut roots: SubmitRootQuery,
    sources: Query<(&SourceEpoch, &ActiveConnectionHandle), With<ConnectionSource>>,
    attachments_query: Query<&Attachments, With<ChatRuntime>>,
    mut submitted: MessageWriter<Submitted>,
    mut effects: MessageWriter<PublishComposerEffect>,
    mut commands: Commands,
) {
    let Ok((generation, selected, mut sequence, mut draft, mut revision)) = roots.single_mut()
    else {
        return;
    };
    let Some(selected) = selected else {
        return;
    };
    let Ok((epoch, active_handle)) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    for intent in intents.read() {
        let mut attachments = Vec::new();
        if let Ok(selected_attachments) = attachments_query.single() {
            for attachment in &selected_attachments.0 {
                attachments.push(AgentAttachment {
                    path: attachment.path.clone(),
                    name: attachment.name.clone(),
                    mime_type: attachment.mime_type.clone(),
                    size: attachment.size,
                });
            }
        }
        if let Some(effect) = revision.clear_effect(&mut draft) {
            effects.write(PublishComposerEffect(effect));
        }
        submitted.write(Submitted);
        commands.spawn((
            Operation::bundle("mobile.chat.submit").unwrap().running(),
            OperationRetained,
            PromptSendOp {
                source_epoch: epoch.0,
                session_generation: generation.0,
            },
            SendPromptJob {
                handle: handle.clone(),
                sid: selected.session_id.clone(),
                client_op_id: sequence.next(),
                text: intent.text.clone(),
                attachments,
            },
        ));
    }
}

fn admit_cancel(
    mut intents: MessageReader<CancelChatIntent>,
    roots: Query<Option<&SelectedSession>, With<MobileRoot>>,
    sources: Query<&ActiveConnectionHandle, With<ConnectionSource>>,
    mut commands: Commands,
) {
    if intents.read().count() == 0 {
        return;
    }
    let Ok(Some(selected)) = roots.single() else {
        return;
    };
    let Ok(active_handle) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    commands.spawn((
        Operation::bundle("mobile.chat.cancel").unwrap().running(),
        OperationRetained,
        ScopedActionOp,
        CancelChatJob {
            handle,
            sid: selected.session_id.clone(),
        },
    ));
}

fn admit_escape(
    mut intents: MessageReader<EscapeChatIntent>,
    mut roots: Query<
        (
            Option<&SelectedSession>,
            &mut ComposerDraft,
            &mut ComposerEffectRevision,
        ),
        With<MobileRoot>,
    >,
    sources: Query<&ActiveConnectionHandle, With<ConnectionSource>>,
    chats: Query<&Conversation, With<ChatRuntime>>,
    mut effects: MessageWriter<PublishComposerEffect>,
    mut commands: Commands,
) {
    if intents.read().count() == 0 {
        return;
    }
    let Ok((selected, mut draft, mut revision)) = roots.single_mut() else {
        return;
    };
    let Some(selected) = selected else {
        return;
    };
    let running = chats
        .single()
        .is_ok_and(|conversation| matches!(conversation.status, RemoteStatus::Streaming));
    if !running && let Some(effect) = revision.clear_effect(&mut draft) {
        effects.write(PublishComposerEffect(effect));
    }
    let Ok(active_handle) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    commands.spawn((
        Operation::bundle("mobile.chat.cancel").unwrap().running(),
        OperationRetained,
        ScopedActionOp,
        CancelChatJob {
            handle,
            sid: selected.session_id.clone(),
        },
    ));
}

fn admit_approve(
    mut intents: MessageReader<ApproveCallIntent>,
    roots: Query<Option<&SelectedSession>, With<MobileRoot>>,
    sources: Query<&ActiveConnectionHandle, With<ConnectionSource>>,
    mut reported: MessageWriter<Reported>,
    mut commands: Commands,
) {
    let Ok(Some(selected)) = roots.single() else {
        return;
    };
    let Ok(active_handle) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    for intent in intents.read() {
        reported.write(Reported(RemoteEvent::Approval { approval: None }));
        commands.spawn((
            Operation::bundle("mobile.chat.approve").unwrap().running(),
            OperationRetained,
            ScopedActionOp,
            ApproveCallJob {
                handle: handle.clone(),
                sid: selected.session_id.clone(),
                call_id: intent.call_id.clone(),
                decision: intent.decision,
            },
        ));
    }
}

fn admit_select_model(
    mut intents: MessageReader<SelectModelIntent>,
    roots: Query<Option<&SelectedSession>, With<MobileRoot>>,
    sources: Query<&ActiveConnectionHandle, With<ConnectionSource>>,
    mut commands: Commands,
) {
    let Ok(Some(selected)) = roots.single() else {
        return;
    };
    let Ok(active_handle) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    for intent in intents.read() {
        commands.spawn((
            Operation::bundle("mobile.chat.select_model")
                .unwrap()
                .running(),
            OperationRetained,
            ScopedActionOp,
            SelectRemoteModelJob {
                handle: handle.clone(),
                sid: selected.session_id.clone(),
                model_id: intent.model_id.clone(),
            },
        ));
    }
}

fn admit_set_effort(
    mut intents: MessageReader<SetEffortIntent>,
    roots: Query<Option<&SelectedSession>, With<MobileRoot>>,
    sources: Query<&ActiveConnectionHandle, With<ConnectionSource>>,
    mut commands: Commands,
) {
    let Ok(Some(selected)) = roots.single() else {
        return;
    };
    let Ok(active_handle) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    for intent in intents.read() {
        commands.spawn((
            Operation::bundle("mobile.chat.set_effort")
                .unwrap()
                .running(),
            OperationRetained,
            ScopedActionOp,
            SetRemoteEffortJob {
                handle: handle.clone(),
                sid: selected.session_id.clone(),
                level: intent.level.clone(),
            },
        ));
    }
}

fn adopt_prompt_send(
    ops: Query<(Entity, &PromptSendOp, &SendPromptReceipt)>,
    roots: Query<&SessionGeneration, With<MobileRoot>>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut reported: MessageWriter<Reported>,
    mut commands: Commands,
) {
    let current_epoch = sources.single().map(|epoch| epoch.0).ok();
    let current_generation = roots.single().map(|generation| generation.0).ok();
    for (entity, op, receipt) in &ops {
        commands
            .entity(entity)
            .remove::<SendPromptReceipt>()
            .remove::<PromptSendOp>()
            .remove::<OperationRetained>();
        let valid = current_epoch == Some(op.source_epoch)
            && current_generation == Some(op.session_generation);
        if valid && let Err(ApiError::Message(message)) = &receipt.0 {
            reported.write(Reported(RemoteEvent::Status {
                status: RemoteStatus::Errored(message.clone()),
            }));
        }
        let state = if receipt.0.is_ok() {
            OperationState::Succeeded
        } else {
            OperationState::Failed
        };
        commands.entity(entity).insert(state);
    }
}

fn adopt_simple_actions(
    cancels: Query<(Entity, &CancelChatReceipt), With<ScopedActionOp>>,
    approves: Query<(Entity, &ApproveCallReceipt), With<ScopedActionOp>>,
    models: Query<(Entity, &SelectRemoteModelReceipt), With<ScopedActionOp>>,
    efforts: Query<(Entity, &SetRemoteEffortReceipt), With<ScopedActionOp>>,
    mut commands: Commands,
) {
    for (entity, receipt) in &cancels {
        if let Err(error) = &receipt.0 {
            tracing::warn!("cancelling failed: {error:?}");
        }
        commands
            .entity(entity)
            .remove::<CancelChatReceipt>()
            .remove::<ScopedActionOp>()
            .remove::<OperationRetained>()
            .insert(if receipt.0.is_ok() {
                OperationState::Succeeded
            } else {
                OperationState::Failed
            });
    }
    for (entity, receipt) in &approves {
        if let Err(error) = &receipt.0 {
            tracing::warn!("approving failed: {error:?}");
        }
        commands
            .entity(entity)
            .remove::<ApproveCallReceipt>()
            .remove::<ScopedActionOp>()
            .remove::<OperationRetained>()
            .insert(if receipt.0.is_ok() {
                OperationState::Succeeded
            } else {
                OperationState::Failed
            });
    }
    for (entity, receipt) in &models {
        if let Err(error) = &receipt.0 {
            tracing::warn!("selecting the model failed: {error:?}");
        }
        commands
            .entity(entity)
            .remove::<SelectRemoteModelReceipt>()
            .remove::<ScopedActionOp>()
            .remove::<OperationRetained>()
            .insert(if receipt.0.is_ok() {
                OperationState::Succeeded
            } else {
                OperationState::Failed
            });
    }
    for (entity, receipt) in &efforts {
        if let Err(error) = &receipt.0 {
            tracing::warn!("setting the effort failed: {error:?}");
        }
        commands
            .entity(entity)
            .remove::<SetRemoteEffortReceipt>()
            .remove::<ScopedActionOp>()
            .remove::<OperationRetained>()
            .insert(if receipt.0.is_ok() {
                OperationState::Succeeded
            } else {
                OperationState::Failed
            });
    }
}

fn poll_models(
    mut roots: Query<
        (
            &SessionGeneration,
            Option<&SelectedSession>,
            &mut ModelPollState,
        ),
        With<MobileRoot>,
    >,
    sources: Query<(Entity, &SourceEpoch, &ActiveConnectionHandle), With<ConnectionSource>>,
    interests: Query<&MountInterest>,
    mut commands: Commands,
) {
    let has_chat_interest = interests
        .iter()
        .any(|interest| interest.kind == MountInterestKind::Chat);
    if !has_chat_interest {
        return;
    }
    let Ok((generation, selected, mut poll)) = roots.single_mut() else {
        return;
    };
    let Some(selected) = selected else {
        return;
    };
    if poll.completed || poll.in_flight || poll.attempts_remaining == 0 {
        return;
    }
    if Instant::now() < poll.next_at {
        return;
    }
    let Ok((source, epoch, active_handle)) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    poll.attempts_remaining -= 1;
    poll.in_flight = true;
    commands.spawn((
        Operation::bundle("mobile.chat.models").unwrap().running(),
        OperationRetained,
        ModelsReadOp {
            source,
            source_epoch: epoch.0,
            session_generation: generation.0,
        },
        ReadModelsJob {
            handle,
            sid: selected.session_id.clone(),
        },
    ));
}

fn adopt_models(
    ops: Query<(Entity, &ModelsReadOp, &ReadModelsReceipt)>,
    mut roots: Query<(&SessionGeneration, &mut ModelPollState), With<MobileRoot>>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    mut models_writer: MessageWriter<Models>,
    mut unauthorized: MessageWriter<SourceUnauthorized>,
    mut commands: Commands,
) {
    for (entity, op, receipt) in &ops {
        commands
            .entity(entity)
            .remove::<ReadModelsReceipt>()
            .remove::<ModelsReadOp>()
            .remove::<OperationRetained>();
        let Ok((generation, mut poll)) = roots.single_mut() else {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        let valid_source = sources
            .get(op.source)
            .is_ok_and(|epoch| epoch.0 == op.source_epoch);
        if !valid_source || generation.0 != op.session_generation {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        poll.in_flight = false;
        match &receipt.error {
            None => {
                poll.completed = true;
                models_writer.write(Models(receipt.to_state()));
                commands.entity(entity).insert(OperationState::Succeeded);
            }
            Some(ApiError::Unauthorized) => {
                poll.completed = true;
                unauthorized.write(SourceUnauthorized {
                    source: op.source,
                    epoch: op.source_epoch,
                });
                commands.entity(entity).insert(OperationState::Failed);
            }
            Some(ApiError::NotFound) => {
                poll.completed = true;
                commands.entity(entity).insert(OperationState::Failed);
            }
            Some(ApiError::Message(_)) => {
                poll.next_at = Instant::now() + ModelPollState::RETRY_INTERVAL;
                commands.entity(entity).insert(OperationState::Failed);
            }
        }
    }
}

fn poll_media(
    mut roots: Query<
        (
            &SessionGeneration,
            Option<&SelectedSession>,
            &mut MediaQueryState,
        ),
        With<MobileRoot>,
    >,
    sources: Query<(&SourceEpoch, &ActiveConnectionHandle), With<ConnectionSource>>,
    mut commands: Commands,
) {
    let Ok((generation, selected, mut media)) = roots.single_mut() else {
        return;
    };
    if !media.dirty || media.query.is_empty() {
        return;
    }
    let Some(selected) = selected else {
        return;
    };
    let Ok((epoch, active_handle)) = sources.single() else {
        return;
    };
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    media.dirty = false;
    commands.spawn((
        Operation::bundle("mobile.chat.media").unwrap().running(),
        OperationRetained,
        MediaReadOp {
            source_epoch: epoch.0,
            session_generation: generation.0,
            request_id: media.request_id,
            query: media.query.clone(),
        },
        ReadMediaJob {
            handle,
            sid: selected.session_id.clone(),
            query: media.query.clone(),
        },
    ));
}

fn adopt_media(
    ops: Query<(Entity, &MediaReadOp, &ReadMediaReceipt)>,
    roots: Query<(Entity, &SessionGeneration, &MediaQueryState), With<MobileRoot>>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    existing_offers: Query<Entity, With<MediaOfferItem>>,
    mut browsed: MessageWriter<Browsed>,
    mut commands: Commands,
) {
    for (entity, op, receipt) in &ops {
        commands
            .entity(entity)
            .remove::<ReadMediaReceipt>()
            .remove::<MediaReadOp>()
            .remove::<OperationRetained>();
        let Ok((root, generation, media)) = roots.single() else {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        let valid_source = sources
            .single()
            .is_ok_and(|epoch| epoch.0 == op.source_epoch);
        let matches = valid_source
            && generation.0 == op.session_generation
            && media.request_id == op.request_id
            && media.query == op.query;
        if !matches {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        match &receipt.0 {
            Ok(entries) => {
                for offer in &existing_offers {
                    commands.entity(offer).despawn();
                }
                for entry in entries {
                    spawn_media_offer(&mut commands, root, op.session_generation, entry);
                }
                browsed.write(Browsed {
                    request_id: op.request_id,
                    query: op.query.clone(),
                    entries: entries.clone(),
                });
                commands.entity(entity).insert(OperationState::Succeeded);
            }
            Err(_) => {
                commands.entity(entity).insert(OperationState::Failed);
            }
        }
    }
}

fn spawn_media_offer(
    commands: &mut Commands,
    root: Entity,
    session_generation: u64,
    entry: &RemoteMediaEntry,
) {
    commands.spawn((
        ChildOf(root),
        MediaOfferItem,
        MediaOfferGeneration(session_generation),
        MediaOfferPath(entry.path.clone()),
        MediaOfferDetails {
            name: entry.name.clone(),
            mime_type: entry.mime_type.clone(),
            size: entry.size,
            is_dir: entry.is_dir,
            preview_data_url: entry.preview_data_url.clone(),
        },
    ));
}

fn poll_team(
    mut roots: Query<&mut TeamPollState, With<MobileRoot>>,
    sources: Query<
        (Entity, &AuthStage, &SourceEpoch, &ActiveConnectionHandle),
        With<ConnectionSource>,
    >,
    interests: Query<&MountInterest>,
    mut commands: Commands,
) {
    let has_team_interest = interests
        .iter()
        .any(|interest| interest.kind == MountInterestKind::Team);
    if !has_team_interest {
        return;
    }
    let Ok(mut poll) = roots.single_mut() else {
        return;
    };
    if poll.in_flight || Instant::now() < poll.next_at {
        return;
    }
    let Ok((source, auth, epoch, active_handle)) = sources.single() else {
        return;
    };
    if *auth != AuthStage::Paired || poll.stopped_for_epoch == Some(epoch.0) {
        return;
    }
    let Some(handle) = active_handle.0.clone() else {
        return;
    };
    poll.in_flight = true;
    commands.spawn((
        Operation::bundle("mobile.team.read").unwrap().running(),
        OperationRetained,
        TeamReadOp {
            source,
            source_epoch: epoch.0,
        },
        ReadTeamJob { handle },
    ));
}

fn adopt_team(
    ops: Query<(Entity, &TeamReadOp, &ReadTeamReceipt)>,
    mut roots: Query<&mut TeamPollState, With<MobileRoot>>,
    sources: Query<&SourceEpoch, With<ConnectionSource>>,
    interests: Query<&MountInterest>,
    mut members_writer: MessageWriter<Members>,
    mut unauthorized: MessageWriter<SourceUnauthorized>,
    mut commands: Commands,
) {
    let has_team_interest = interests
        .iter()
        .any(|interest| interest.kind == MountInterestKind::Team);
    for (entity, op, receipt) in &ops {
        commands
            .entity(entity)
            .remove::<ReadTeamReceipt>()
            .remove::<TeamReadOp>()
            .remove::<OperationRetained>();
        let Ok(mut poll) = roots.single_mut() else {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        };
        let valid_source = sources
            .get(op.source)
            .is_ok_and(|epoch| epoch.0 == op.source_epoch);
        if !valid_source {
            poll.in_flight = false;
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        poll.in_flight = false;
        poll.next_at = Instant::now() + TeamPollState::POLL_INTERVAL;
        match &receipt.0 {
            Ok(members) => {
                if has_team_interest {
                    members_writer.write(Members(members.clone()));
                }
                commands.entity(entity).insert(OperationState::Succeeded);
            }
            Err(ApiError::Unauthorized) => {
                poll.stopped_for_epoch = Some(op.source_epoch);
                unauthorized.write(SourceUnauthorized {
                    source: op.source,
                    epoch: op.source_epoch,
                });
                commands.entity(entity).insert(OperationState::Failed);
            }
            Err(ApiError::NotFound) => {
                poll.stopped_for_epoch = Some(op.source_epoch);
                commands.entity(entity).insert(OperationState::Failed);
            }
            Err(ApiError::Message(_)) => {
                commands.entity(entity).insert(OperationState::Failed);
            }
        }
    }
}
