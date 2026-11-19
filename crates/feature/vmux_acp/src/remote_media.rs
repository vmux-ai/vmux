use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use vmux_api::protocol::{AgentAttachment, ApprovalDecision, SharedFailure, SharedResponse};
use vmux_api::session::RemoteMediaEntry;
use vmux_ecs::{EntityTarget, OperationTarget, Uuid};
use vmux_session::SessionId;

use super::acp::AcpInput;
use super::remote::{Live, RemoteSet, Response, Waiting};
use super::remote_file_io::{Directory, DirectoryBatch, FileFacts, Home, Thumbnail};
use super::remote_io::{Accepted, Joined, Loan, Output};
use super::service_driver::AcpService;

pub(super) struct RemoteMediaPlugin;

impl Plugin for RemoteMediaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                collect_sessions,
                collect_lookup,
                collect_input,
                collect_home,
                collect_prompt_file,
                collect_media_file,
                collect_directory,
                collect_batch,
                collect_details,
                collect_preview,
            )
                .chain()
                .in_set(RemoteSet::Collect),
        )
        .add_systems(
            Update,
            (
                validate_prompt,
                submit_simple,
                submit_home,
                submit_prompt_file,
                submit_media_file,
                submit_directory,
                submit_batch,
                submit_details,
                submit_preview,
                submit_prompt,
            )
                .chain()
                .in_set(RemoteSet::Submit),
        )
        .add_systems(Update, close_directory.in_set(RemoteSet::Clean));
    }
}

#[derive(Component)]
pub(super) struct Sessions;
#[derive(Component)]
pub(super) struct Attach;
#[derive(Component)]
pub(super) struct Cancel;
#[derive(Component)]
pub(super) struct Approve {
    pub call_id: String,
    pub decision: ApprovalDecision,
}

#[derive(Component)]
pub(super) struct Prompt {
    text: String,
    context: Option<String>,
    paths: Vec<PathBuf>,
    attachments: Vec<AgentAttachment>,
    preferred_mode: Option<String>,
    total: u64,
    phase: PromptPhase,
}

enum PromptPhase {
    Validate,
    Home,
    Files,
    Input,
}

impl Prompt {
    const MAX_PROMPT_BYTES: usize = 64 * 1024;
    const MAX_ATTACHMENTS: usize = 16;
    const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
    const MAX_ATTACHMENT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

    pub(super) fn new(
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
        preferred_mode: Option<String>,
    ) -> Self {
        let mut paths = Vec::new();
        for attachment in attachments {
            paths.push(PathBuf::from(attachment.path));
        }
        Self {
            text,
            context,
            paths,
            attachments: Vec::new(),
            preferred_mode,
            total: 0,
            phase: PromptPhase::Validate,
        }
    }
}

#[derive(Component)]
pub(super) struct Media {
    query: String,
    filter: String,
    directory: Option<PathBuf>,
    entries: Vec<RemoteMediaEntry>,
    preview: usize,
    remaining: u64,
    phase: MediaPhase,
}

enum MediaPhase {
    Lookup,
    Home,
    Candidate,
    Directory,
    Open,
    Read,
    Details,
    Preview,
}

impl Media {
    const MAX_MEDIA_QUERY_BYTES: usize = 4 * 1024;
    const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;
    const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
    const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 512;

    pub(super) fn new(query: String) -> Self {
        Self {
            query,
            filter: String::new(),
            directory: None,
            entries: Vec::new(),
            preview: 0,
            remaining: Media::MEDIA_THUMBNAIL_TOTAL_LIMIT,
            phase: MediaPhase::Lookup,
        }
    }

    fn candidate(&self, home: &Path) -> PathBuf {
        if let Some(rest) = self.query.strip_prefix("file://") {
            return Self::decode(rest);
        }
        if let Some(rest) = self.query.strip_prefix("~/") {
            return home.join(Self::decode(rest));
        }
        if self.query == "~" {
            return home.to_path_buf();
        }
        let path = Self::decode(&self.query);
        if path.is_absolute() {
            path
        } else {
            home.join(path)
        }
    }

    fn decode(value: &str) -> PathBuf {
        let bytes = value.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%'
                && index + 2 < bytes.len()
                && let (Some(high), Some(low)) = (
                    char::from(bytes[index + 1]).to_digit(16),
                    char::from(bytes[index + 2]).to_digit(16),
                )
            {
                decoded.push(((high << 4) | low) as u8);
                index += 3;
            } else {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
        PathBuf::from(String::from_utf8_lossy(&decoded).into_owned())
    }
}

#[derive(Component)]
struct DirectoryLease(Arc<Directory>);
#[derive(Component)]
struct SessionsIo;
#[derive(Component)]
struct LookupIo;
#[derive(Component)]
struct InputIo;
#[derive(Component)]
struct HomeIo;
#[derive(Component)]
struct PromptFileIo;
#[derive(Component)]
struct MediaFileIo;
#[derive(Component)]
struct DirectoryIo;
#[derive(Component)]
struct BatchIo;
#[derive(Component)]
struct PreviewIo(usize);
#[derive(Component)]
struct DetailsIo;

fn validate_prompt(
    mut prompts: Query<(Entity, &mut Prompt), Without<Response>>,
    mut commands: Commands,
) {
    for (entity, mut prompt) in &mut prompts {
        if !matches!(prompt.phase, PromptPhase::Validate) {
            continue;
        }
        if prompt.text.trim().is_empty()
            || prompt.text.len() > Prompt::MAX_PROMPT_BYTES
            || prompt.paths.len() > Prompt::MAX_ATTACHMENTS
        {
            commands
                .entity(entity)
                .insert(Response(SharedResponse::Failed(SharedFailure::Invalid)));
        } else {
            prompt.phase = PromptPhase::Home;
        }
    }
}

fn submit_simple(
    requests: Query<
        (
            Entity,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
            Option<&SessionId>,
            Has<Sessions>,
            Has<Attach>,
            Has<Cancel>,
            Option<&Approve>,
            Option<&Media>,
        ),
        (Without<Waiting>, Without<Response>),
    >,
    services: Query<&AcpService>,
    mut commands: Commands,
) {
    for (entity, owner, uuid, loan, sid, sessions, attach, cancel, approve, media) in &requests {
        let Ok(service) = services.get(owner.entity()) else {
            continue;
        };
        let sessions_api = service.sessions().clone();
        let accepted = Accepted {
            owner: owner.entity(),
            uuid: uuid.clone(),
        };
        if sessions {
            let native = commands
                .spawn((
                    Output::spawn(loan, accepted, async move {
                        sessions_api.remote_sessions().await
                    }),
                    OperationTarget::new(entity),
                ))
                .id();
            commands.entity(native).insert(SessionsIo);
            commands.entity(entity).insert(Waiting(native));
        } else if attach || media.is_some_and(|media| matches!(media.phase, MediaPhase::Lookup)) {
            let Some(sid) = sid else {
                continue;
            };
            let sid = sid.0.clone();
            let native = commands
                .spawn((
                    Output::spawn(loan, accepted, async move {
                        sessions_api.remote_session(sid).await
                    }),
                    OperationTarget::new(entity),
                ))
                .id();
            commands.entity(native).insert(LookupIo);
            commands.entity(entity).insert(Waiting(native));
        } else if cancel {
            let Some(sid) = sid else {
                continue;
            };
            let sid = sid.0.clone();
            let native = commands
                .spawn((
                    Output::spawn(loan, accepted, async move {
                        sessions_api.input(sid, AcpInput::Cancel).await
                    }),
                    OperationTarget::new(entity),
                ))
                .id();
            commands.entity(native).insert(InputIo);
            commands.entity(entity).insert(Waiting(native));
        } else if let Some(approve) = approve {
            let Some(sid) = sid else {
                continue;
            };
            let sid = sid.0.clone();
            let call_id = approve.call_id.clone();
            let decision = approve.decision;
            let native = commands
                .spawn((
                    Output::spawn(loan, accepted, async move {
                        sessions_api
                            .input(sid, AcpInput::Approve { call_id, decision })
                            .await
                    }),
                    OperationTarget::new(entity),
                ))
                .id();
            commands.entity(native).insert(InputIo);
            commands.entity(entity).insert(Waiting(native));
        }
    }
}

fn collect_sessions(
    mut tasks: Query<
        (
            Entity,
            &mut Output<Vec<vmux_api::session::RemoteSession>>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<SessionsIo>,
    >,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        let result = output.take(joined);
        if let Some(target) = live.accepts(entity, accepted, target) {
            let response = match result {
                Ok(sessions) => SharedResponse::Sessions(sessions),
                Err(_) => SharedResponse::Failed(SharedFailure::Internal),
            };
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(response));
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn collect_lookup(
    mut tasks: Query<
        (
            Entity,
            &mut Output<Option<vmux_api::session::RemoteSession>>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<LookupIo>,
    >,
    mut media: Query<&mut Media>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        let found = matches!(output.take(joined), Ok(Some(_)));
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            if !found {
                commands
                    .entity(target)
                    .insert(Response(SharedResponse::Failed(SharedFailure::NotFound)));
            } else if let Ok(mut media) = media.get_mut(target) {
                if media.query.len() > Media::MAX_MEDIA_QUERY_BYTES {
                    commands
                        .entity(target)
                        .insert(Response(SharedResponse::Failed(SharedFailure::Invalid)));
                } else {
                    media.phase = MediaPhase::Home;
                }
            } else {
                commands.entity(target).insert(Response(SharedResponse::Ok));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn collect_input(
    mut tasks: Query<
        (
            Entity,
            &mut Output<bool>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<InputIo>,
    >,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        let response = match output.take(joined) {
            Ok(true) => SharedResponse::Ok,
            Ok(false) => SharedResponse::Failed(SharedFailure::NotFound),
            Err(_) => SharedResponse::Failed(SharedFailure::Internal),
        };
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(response));
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn submit_home(
    requests: Query<
        (
            Entity,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
            Option<&Prompt>,
            Option<&Media>,
        ),
        (Without<Waiting>, Without<Response>),
    >,
    mut commands: Commands,
) {
    for (entity, owner, uuid, loan, prompt, media) in &requests {
        if !prompt.is_some_and(|prompt| matches!(prompt.phase, PromptPhase::Home))
            && !media.is_some_and(|media| matches!(media.phase, MediaPhase::Home))
        {
            continue;
        }
        let native = commands
            .spawn((
                Output::spawn_blocking(
                    loan,
                    Accepted {
                        owner: owner.entity(),
                        uuid: uuid.clone(),
                    },
                    Home::read,
                ),
                OperationTarget::new(entity),
            ))
            .id();
        commands.entity(native).insert(HomeIo);
        commands.entity(entity).insert(Waiting(native));
    }
}

fn collect_home(
    mut tasks: Query<
        (
            Entity,
            &mut Output<Result<Home, String>>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<HomeIo>,
    >,
    mut prompts: Query<&mut Prompt>,
    mut media: Query<&mut Media>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        let result = output.take(joined);
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            if let Ok(Ok(home)) = result {
                commands.entity(target).insert(home);
                if let Ok(mut prompt) = prompts.get_mut(target) {
                    prompt.phase = PromptPhase::Files;
                }
                if let Ok(mut media) = media.get_mut(target) {
                    media.phase = MediaPhase::Candidate;
                }
            } else {
                let response = if prompts.contains(target) {
                    SharedResponse::Failed(SharedFailure::Invalid)
                } else {
                    SharedResponse::Media(Vec::new())
                };
                commands.entity(target).insert(Response(response));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn submit_prompt_file(
    mut prompts: Query<
        (
            Entity,
            &mut Prompt,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
            &Home,
        ),
        (Without<Waiting>, Without<Response>),
    >,
    mut commands: Commands,
) {
    for (entity, mut prompt, owner, uuid, loan, _) in &mut prompts {
        if !matches!(prompt.phase, PromptPhase::Files) {
            continue;
        }
        if prompt.attachments.len() == prompt.paths.len() {
            prompt.phase = PromptPhase::Input;
            continue;
        }
        let path = prompt.paths[prompt.attachments.len()].clone();
        let native = commands
            .spawn((
                Output::spawn_blocking(
                    loan,
                    Accepted {
                        owner: owner.entity(),
                        uuid: uuid.clone(),
                    },
                    move || FileFacts::read(path),
                ),
                OperationTarget::new(entity),
            ))
            .id();
        commands.entity(native).insert(PromptFileIo);
        commands.entity(entity).insert(Waiting(native));
    }
}

fn collect_prompt_file(
    mut tasks: Query<
        (
            Entity,
            &mut Output<FileFacts>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<PromptFileIo>,
    >,
    mut prompts: Query<(&mut Prompt, &Home)>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            let result = output.take(joined);
            let mut valid = false;
            if let Ok(facts) = result
                && let (Some(path), Some(metadata)) = (facts.canonical, facts.metadata)
                && let Ok((mut prompt, home)) = prompts.get_mut(target)
                && path.starts_with(&home.canonical)
                && metadata.is_file()
                && metadata.len() <= Prompt::MAX_ATTACHMENT_BYTES
                && let Some(total) = prompt.total.checked_add(metadata.len())
                && total <= Prompt::MAX_ATTACHMENT_TOTAL_BYTES
                && let Some(name) = path.file_name()
            {
                prompt.total = total;
                prompt.attachments.push(AgentAttachment {
                    name: name.to_string_lossy().into_owned(),
                    mime_type: FileFacts::mime(&path),
                    path: path.to_string_lossy().into_owned(),
                    size: metadata.len(),
                });
                valid = true;
            }
            if !valid {
                commands
                    .entity(target)
                    .insert(Response(SharedResponse::Failed(SharedFailure::Invalid)));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn submit_prompt(
    prompts: Query<
        (
            Entity,
            &Prompt,
            &SessionId,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
        ),
        (Without<Waiting>, Without<Response>),
    >,
    services: Query<&AcpService>,
    mut commands: Commands,
) {
    for (entity, prompt, sid, owner, uuid, loan) in &prompts {
        if !matches!(prompt.phase, PromptPhase::Input) {
            continue;
        }
        let Ok(service) = services.get(owner.entity()) else {
            continue;
        };
        let sessions = service.sessions().clone();
        let sid = sid.0.clone();
        let input = AcpInput::User {
            text: prompt.text.clone(),
            context: prompt.context.clone(),
            attachments: prompt.attachments.clone(),
            preferred_mode: prompt.preferred_mode.clone(),
        };
        let native = commands
            .spawn((
                Output::spawn(
                    loan,
                    Accepted {
                        owner: owner.entity(),
                        uuid: uuid.clone(),
                    },
                    async move { sessions.input(sid, input).await },
                ),
                OperationTarget::new(entity),
            ))
            .id();
        commands.entity(native).insert(InputIo);
        commands.entity(entity).insert(Waiting(native));
    }
}

fn submit_media_file(
    media: Query<
        (
            Entity,
            &Media,
            &Home,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
        ),
        (Without<Waiting>, Without<Response>),
    >,
    mut commands: Commands,
) {
    for (entity, media, home, owner, uuid, loan) in &media {
        let path = match media.phase {
            MediaPhase::Candidate => media.candidate(&home.raw),
            MediaPhase::Directory => media.directory.clone().unwrap(),
            _ => continue,
        };
        let native = commands
            .spawn((
                Output::spawn_blocking(
                    loan,
                    Accepted {
                        owner: owner.entity(),
                        uuid: uuid.clone(),
                    },
                    move || FileFacts::read(path),
                ),
                OperationTarget::new(entity),
            ))
            .id();
        commands.entity(native).insert(MediaFileIo);
        commands.entity(entity).insert(Waiting(native));
    }
}

fn collect_media_file(
    mut tasks: Query<
        (
            Entity,
            &mut Output<FileFacts>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<MediaFileIo>,
    >,
    mut media: Query<(&mut Media, &Home)>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            let mut valid = false;
            if let Ok(facts) = output.take(joined)
                && let Ok((mut media, home)) = media.get_mut(target)
            {
                let is_directory = media.query.is_empty()
                    || media.query.ends_with('/')
                    || facts.metadata.as_ref().is_some_and(Metadata::is_dir);
                if matches!(media.phase, MediaPhase::Candidate) && !is_directory {
                    media.directory = Some(
                        facts
                            .requested
                            .parent()
                            .map(Path::to_path_buf)
                            .unwrap_or_else(|| home.raw.clone()),
                    );
                    media.filter = facts
                        .requested
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    media.phase = MediaPhase::Directory;
                    valid = true;
                } else if let Some(directory) = facts.canonical
                    && directory.starts_with(&home.canonical)
                {
                    media.directory = Some(directory);
                    media.phase = MediaPhase::Open;
                    valid = true;
                }
            }
            if !valid {
                commands
                    .entity(target)
                    .insert(Response(SharedResponse::Media(Vec::new())));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn submit_directory(
    media: Query<
        (Entity, &Media, &EntityTarget<AcpService>, &Uuid, &Loan),
        (Without<Waiting>, Without<Response>),
    >,
    mut commands: Commands,
) {
    for (entity, media, owner, uuid, loan) in &media {
        if !matches!(media.phase, MediaPhase::Open) {
            continue;
        }
        let directory = media.directory.clone().unwrap();
        let native = commands
            .spawn((
                Output::spawn_blocking(
                    loan,
                    Accepted {
                        owner: owner.entity(),
                        uuid: uuid.clone(),
                    },
                    move || Directory::open(directory),
                ),
                OperationTarget::new(entity),
            ))
            .id();
        commands.entity(native).insert(DirectoryIo);
        commands.entity(entity).insert(Waiting(native));
    }
}

fn collect_directory(
    mut tasks: Query<
        (
            Entity,
            &mut Output<Result<Directory, String>>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<DirectoryIo>,
    >,
    mut media: Query<(&mut Media, &Home)>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            let mut valid = false;
            if let Ok(Ok(directory)) = output.take(joined)
                && let Ok((mut media, home)) = media.get_mut(target)
                && directory.path.starts_with(&home.canonical)
            {
                media.phase = MediaPhase::Read;
                commands
                    .entity(target)
                    .insert(DirectoryLease(Arc::new(directory)));
                valid = true;
            }
            if !valid {
                commands
                    .entity(target)
                    .insert(Response(SharedResponse::Media(Vec::new())));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn submit_batch(
    media: Query<
        (
            Entity,
            &Media,
            &DirectoryLease,
            &EntityTarget<AcpService>,
            &Uuid,
            &Loan,
        ),
        (Without<Waiting>, Without<Response>),
    >,
    mut commands: Commands,
) {
    for (entity, media, directory, owner, uuid, loan) in &media {
        if !matches!(media.phase, MediaPhase::Read) {
            continue;
        }
        let directory = directory.0.clone();
        let native = commands
            .spawn((
                Output::spawn_blocking(
                    loan,
                    Accepted {
                        owner: owner.entity(),
                        uuid: uuid.clone(),
                    },
                    move || directory.batch(256),
                ),
                OperationTarget::new(entity),
            ))
            .id();
        commands.entity(native).insert(BatchIo);
        commands.entity(entity).insert(Waiting(native));
    }
}

fn collect_batch(
    mut tasks: Query<
        (
            Entity,
            &mut Output<Result<DirectoryBatch, String>>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<BatchIo>,
    >,
    mut media: Query<(&mut Media, &Home)>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            if let Ok(Ok(batch)) = output.take(joined)
                && let Ok((mut media, home)) = media.get_mut(target)
            {
                for entry in batch.entries {
                    if entry.name.starts_with('.')
                        || (!media.filter.is_empty()
                            && !entry.name.to_ascii_lowercase().contains(&media.filter))
                    {
                        continue;
                    }
                    let Some(kind) = entry.file_type else {
                        continue;
                    };
                    let is_dir = kind.is_dir();
                    let mime_type = if is_dir {
                        String::new()
                    } else {
                        FileFacts::mime(&entry.path)
                    };
                    if !is_dir
                        && !mime_type.starts_with("image/")
                        && !mime_type.starts_with("audio/")
                        && !mime_type.starts_with("video/")
                        && mime_type != "application/pdf"
                    {
                        continue;
                    }
                    let parent = match entry
                        .path
                        .parent()
                        .and_then(|parent| parent.strip_prefix(&home.canonical).ok())
                    {
                        Some(parent) if !parent.as_os_str().is_empty() => {
                            format!("~/{}", parent.to_string_lossy())
                        }
                        _ => "~".to_string(),
                    };
                    media.entries.push(RemoteMediaEntry {
                        path: entry.path.to_string_lossy().into_owned(),
                        name: entry.name,
                        parent,
                        mime_type,
                        size: 0,
                        is_dir,
                        preview_data_url: String::new(),
                    });
                }
                media.entries.sort_by(|left, right| {
                    right.is_dir.cmp(&left.is_dir).then_with(|| {
                        left.name
                            .to_ascii_lowercase()
                            .cmp(&right.name.to_ascii_lowercase())
                    })
                });
                media.entries.truncate(100);
                if batch.ended {
                    media.phase = MediaPhase::Details;
                    commands.entity(target).remove::<DirectoryLease>();
                }
            } else {
                commands
                    .entity(target)
                    .insert(Response(SharedResponse::Media(Vec::new())));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn submit_preview(
    mut media: Query<
        (Entity, &mut Media, &EntityTarget<AcpService>, &Uuid, &Loan),
        (Without<Waiting>, Without<Response>),
    >,
    mut commands: Commands,
) {
    for (entity, mut media, owner, uuid, loan) in &mut media {
        if !matches!(media.phase, MediaPhase::Preview) {
            continue;
        }
        let mut pending = false;
        while media.preview < media.entries.len() {
            let index = media.preview;
            media.preview += 1;
            let entry = &media.entries[index];
            if entry.is_dir
                || !entry.mime_type.starts_with("image/")
                || entry.size > media.remaining
                || entry.size > Media::MEDIA_THUMBNAIL_SOURCE_LIMIT
            {
                continue;
            }
            let Some(mime) = vmux_api::media::MediaKind::image_mime(&entry.path) else {
                continue;
            };
            if mime == "image/svg+xml" || mime == "image/avif" {
                continue;
            }
            let path = PathBuf::from(&entry.path);
            let native = commands
                .spawn((
                    Output::spawn_blocking(
                        loan,
                        Accepted {
                            owner: owner.entity(),
                            uuid: uuid.clone(),
                        },
                        move || {
                            Thumbnail::read(
                                path,
                                Media::MEDIA_THUMBNAIL_SOURCE_LIMIT,
                                Media::MEDIA_THUMBNAIL_MAX_EDGE,
                            )
                        },
                    ),
                    OperationTarget::new(entity),
                ))
                .id();
            commands.entity(native).insert(PreviewIo(index));
            commands.entity(entity).insert(Waiting(native));
            pending = true;
            break;
        }
        if !pending {
            commands
                .entity(entity)
                .insert(Response(SharedResponse::Media(std::mem::take(
                    &mut media.entries,
                ))));
        }
    }
}

fn collect_preview(
    mut tasks: Query<(
        Entity,
        &mut Output<String>,
        &Joined,
        &Accepted,
        Option<&OperationTarget>,
        &PreviewIo,
    )>,
    mut media: Query<&mut Media>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target, preview) in &mut tasks {
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            if let Ok(data) = output.take(joined)
                && !data.is_empty()
                && let Ok(mut media) = media.get_mut(target)
                && let Some(entry) = media.entries.get_mut(preview.0)
            {
                let size = entry.size;
                entry.preview_data_url = data;
                media.remaining = media.remaining.saturating_sub(size);
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}

fn close_directory(
    operations: Query<Entity, (With<Response>, Without<Waiting>)>,
    mut commands: Commands,
) {
    for entity in &operations {
        commands.entity(entity).remove::<(
            DirectoryLease,
            Prompt,
            Media,
            Home,
            Sessions,
            Attach,
            Cancel,
            Approve,
            SessionId,
        )>();
    }
}

fn submit_details(
    media: Query<
        (Entity, &Media, &EntityTarget<AcpService>, &Uuid, &Loan),
        (Without<Waiting>, Without<Response>),
    >,
    mut commands: Commands,
) {
    for (entity, media, owner, uuid, loan) in &media {
        if !matches!(media.phase, MediaPhase::Details) {
            continue;
        }
        let mut paths = Vec::new();
        for (index, entry) in media.entries.iter().enumerate() {
            if !entry.is_dir {
                paths.push((index, PathBuf::from(&entry.path)));
            }
        }
        let native = commands
            .spawn((
                Output::spawn_blocking(
                    loan,
                    Accepted {
                        owner: owner.entity(),
                        uuid: uuid.clone(),
                    },
                    move || FileFacts::batch(paths),
                ),
                OperationTarget::new(entity),
            ))
            .id();
        commands.entity(native).insert(DetailsIo);
        commands.entity(entity).insert(Waiting(native));
    }
}

fn collect_details(
    mut tasks: Query<
        (
            Entity,
            &mut Output<Vec<(usize, FileFacts)>>,
            &Joined,
            &Accepted,
            Option<&OperationTarget>,
        ),
        With<DetailsIo>,
    >,
    mut media: Query<&mut Media>,
    live: Live,
    mut commands: Commands,
) {
    for (entity, mut output, joined, accepted, target) in &mut tasks {
        if let Some(target) = live.accepts(entity, accepted, target) {
            commands.entity(target).remove::<Waiting>();
            if let Ok(facts) = output.take(joined)
                && let Ok(mut media) = media.get_mut(target)
            {
                for (index, facts) in facts {
                    if let Some(entry) = media.entries.get_mut(index) {
                        entry.size = facts
                            .metadata
                            .map(|metadata| metadata.len())
                            .unwrap_or_default();
                    }
                }
                media.phase = MediaPhase::Preview;
            } else {
                commands
                    .entity(target)
                    .insert(Response(SharedResponse::Failed(SharedFailure::Internal)));
            }
        } else if let Some(target) = live.matching(entity, accepted, target) {
            commands
                .entity(target)
                .remove::<Waiting>()
                .insert(Response(SharedResponse::Failed(SharedFailure::NoDesktop)));
        }
        commands.entity(entity).despawn();
    }
}
