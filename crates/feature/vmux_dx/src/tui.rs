use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_ecs::{CreatedAt, Operation, OperationState};
use vmux_job::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};

use crate::debug_io::{Flight, Task};
use crate::policy::GIB;
use crate::repository::Checkout;
use crate::signals::Signals;
use crate::steps::StorageCleanStep;
use crate::tui_io::{ConfirmOutcome, LiveWriter, PromptConfirm, RunCockpit, Terminal, ThemeStore};
use crate::tui_render::{CockpitAction, NoticeStyle, RecoveryHint, Theme, ThemePreset};
use crate::{
    Accepted, Artifact, Completed, ExecutionSet, Predecessor, Running, Settings, Step, Tail,
};

pub(crate) struct TuiPlugin;

impl bevy_app::Plugin for TuiPlugin {
    fn build(&self, app: &mut App) {
        let (interactive, color, width, theme) = Terminal::probe();
        app.insert_resource(TerminalMode {
            interactive,
            color,
            width,
            theme,
        })
        .insert_resource(Hud::new())
        .add_message::<StorageReported>()
        .add_message::<Notice>()
        .add_message::<CleanPreviewed>()
        .add_message::<ArtifactObserved>()
        .add_plugins((
            JobPlugin::<Task<RunCockpit>>::default(),
            JobPlugin::<Task<PromptConfirm>>::default(),
        ))
        .configure_sets(
            Update,
            (
                TuiSet::Cockpit
                    .in_set(JobSet::Apply)
                    .after(crate::Stage::Initialize)
                    .before(crate::repository_selection::Stage::Resolve),
                TuiSet::Prompt
                    .in_set(JobSet::Apply)
                    .after(crate::storage_runtime::Stage::Release),
                TuiSet::Present
                    .after(TuiSet::Prompt)
                    .after(ExecutionSet::Finish)
                    .after(crate::tooling::Stage::Finish),
            ),
        )
        .add_systems(
            Update,
            (start_cockpit, cancel_cockpit, finish_cockpit)
                .chain()
                .in_set(TuiSet::Cockpit),
        )
        .add_systems(
            Update,
            (prompt_clean, cancel_prompt, finish_prompt)
                .chain()
                .in_set(TuiSet::Prompt),
        )
        .add_systems(
            Update,
            (
                track_sessions,
                observe_artifacts,
                present_storage,
                present_notices,
                track_steps,
                animate,
                present_results,
            )
                .chain()
                .in_set(TuiSet::Present),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TuiSet {
    Cockpit,
    Prompt,
    Present,
}

#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct TerminalMode {
    pub interactive: bool,
    pub color: bool,
    pub width: u16,
    pub theme: ThemePreset,
}

impl TerminalMode {
    fn theme(self) -> Theme {
        Theme::new(self.theme, self.color, self.width)
    }
}

#[derive(Resource)]
struct Hud {
    writer: LiveWriter,
    frame: usize,
    last_tick: Instant,
    active_step: Option<Entity>,
    active_passthrough: bool,
}

impl Hud {
    fn new() -> Self {
        Self {
            writer: LiveWriter::new(),
            frame: 0,
            last_tick: Instant::now(),
            active_step: None,
            active_passthrough: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoragePhase {
    Check,
    Preflight,
    Finish,
    Report,
    CleanPreview,
    CleanApplied,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct StorageReported {
    pub root: Entity,
    pub free_bytes: u64,
    pub cache_bytes: Option<u64>,
    pub warning: Option<String>,
    pub phase: StoragePhase,
    pub profile: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeTone {
    Info,
    Warning,
    Stdout,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct Notice {
    pub root: Option<Entity>,
    pub tone: NoticeTone,
    pub text: String,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct CleanPreviewed {
    pub root: Entity,
    pub reclaimable_count: usize,
    pub free_bytes: u64,
    pub cache_bytes: u64,
    pub targets: Vec<PathBuf>,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct ArtifactObserved {
    pub root: Entity,
    pub path: PathBuf,
    pub commit: Option<String>,
}

#[derive(Component)]
pub(crate) struct PassthroughStep;

#[derive(Component)]
pub(crate) struct CleanConfirming(pub Vec<PathBuf>);

#[derive(Component)]
pub(crate) struct FailedDiagnostic(pub String);

#[derive(Component)]
struct HudSession {
    command: String,
    branch: Option<String>,
    commit: Option<String>,
    profile: Option<String>,
    artifact: Option<PathBuf>,
    free_bytes: Option<u64>,
    pending_check_storage: Option<String>,
    preflight_storage_reported: bool,
    clean_applied: bool,
    last_step_detail: Option<String>,
    started: Instant,
    header_shown: bool,
    silent: bool,
}

impl HudSession {
    fn is_silent(invocation: &CliInvocation, has_dispatch: bool) -> bool {
        has_dispatch
            || invocation.is("dx.internal")
            || invocation.is("dx.ci-scope")
            || invocation.is("dx.ci-native")
            || invocation.is("dx.help")
            || invocation.is("dx.update")
            || invocation.is("dx.tui")
            || invocation.command.starts_with("dx.ci.")
    }

    fn from_invocation(invocation: &CliInvocation, has_dispatch: bool) -> Self {
        let silent = Self::is_silent(invocation, has_dispatch);
        let command = Self::display_command(&invocation.command);
        let branch = invocation.value("branch").map(str::to_owned);
        let profile = invocation.value("profile").map(str::to_owned);
        Self {
            command,
            branch,
            commit: None,
            profile,
            artifact: None,
            free_bytes: None,
            pending_check_storage: None,
            preflight_storage_reported: false,
            clean_applied: false,
            last_step_detail: None,
            started: Instant::now(),
            header_shown: false,
            silent,
        }
    }

    fn retarget(&mut self, invocation: &CliInvocation) {
        self.command = Self::display_command(&invocation.command);
        self.branch = invocation.value("branch").map(str::to_owned);
        self.profile = invocation.value("profile").map(str::to_owned);
        self.silent = Self::is_silent(invocation, false);
        self.started = Instant::now();
    }

    fn display_command(id: &str) -> String {
        match id {
            "dx.desktop" => "desktop".into(),
            "dx.run" => "run".into(),
            "dx.build" => "build".into(),
            "dx.serve" => "serve".into(),
            "dx.stop" => "stop".into(),
            "dx.test" => "test".into(),
            "dx.lint" => "lint".into(),
            "dx.doctor" => "doctor".into(),
            "dx.setup" => "setup".into(),
            "dx.reset-layout" => "reset-layout".into(),
            "dx.cache.status" => "cache status".into(),
            "dx.cache.clean" => "cache clean".into(),
            other => other.trim_start_matches("dx.").replace('.', " "),
        }
    }
}

#[derive(Component)]
struct StepTiming {
    started: Instant,
    announced: bool,
    reported_done: bool,
}

#[derive(Component)]
struct CockpitJob(Entity);

#[derive(Component)]
struct PromptJob(Entity);

#[derive(Component)]
struct ResultPresented;

type IncomingCockpits<'w, 's> =
    Query<'w, 's, (Entity, &'static CliInvocation), (Without<CockpitJob>, Without<CliResult>)>;

type FinishedCockpits<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobResult<Task<RunCockpit>>,
        &'static JobOwner<Task<RunCockpit>>,
        &'static ChildOf,
    ),
>;

type FinishedPrompts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobResult<Task<PromptConfirm>>,
        &'static JobOwner<Task<PromptConfirm>>,
        &'static ChildOf,
    ),
>;

type SessionCandidates<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        Option<&'static crate::workspace::Dispatch>,
        Option<&'static Checkout>,
        Option<&'static Settings>,
        Has<Accepted>,
        Option<&'static mut HudSession>,
    ),
    Without<CliResult>,
>;

type RunningSteps<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Name,
        &'static ChildOf,
        Has<PassthroughStep>,
        Has<Running>,
        Has<Completed>,
        &'static OperationState,
        Option<&'static mut StepTiming>,
    ),
    With<Step>,
>;

type ResultOperations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut CliResult,
        Option<&'static HudSession>,
        Option<&'static FailedDiagnostic>,
        Option<&'static Children>,
    ),
    (Added<CliResult>, Without<ResultPresented>),
>;

fn start_cockpit(
    requests: IncomingCockpits,
    mut mode: ResMut<TerminalMode>,
    mut hud: ResMut<Hud>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    if cfg!(feature = "runtime-cli") {
        return;
    }
    for (entity, invocation) in &requests {
        if !invocation.is("dx.tui") {
            continue;
        }
        if invocation.flag("list-themes") {
            let output = mode.theme().theme_list(mode.theme);
            hud.writer.write_stdout(&output);
            commands.entity(entity).insert(CliResult::default());
            continue;
        }
        if let Some(name) = invocation.value("theme") {
            let Some(preset) = ThemePreset::parse(name) else {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(format!("Unknown TUI theme: {name}"))));
                continue;
            };
            mode.theme = preset;
            if let Err(error) = ThemeStore::save(preset) {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(format!("Failed to save TUI theme: {error}"))));
                continue;
            }
            if !mode.interactive {
                hud.writer
                    .write_stdout(&format!("CLI TUI theme set to {}\n", preset.id()));
                commands.entity(entity).insert(CliResult::default());
                continue;
            }
        }
        if !mode.interactive {
            commands.entity(entity).insert(CliResult(Err(
                "Interactive TUI requires a terminal (TTY); run explicit subcommands like vmux desktop, vmux build, or vmux cache status".into(),
            )));
            continue;
        }
        let root = match invocation.value_os("root") {
            Some(path) => PathBuf::from(path),
            None => match std::env::current_dir() {
                Ok(path) => path,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(CliResult(Err(error.to_string())));
                    continue;
                }
            },
        };
        let job = RunCockpit {
            root,
            color: mode.color,
            theme: mode.theme,
        };
        let (task, flight) = Task::new(job.clone(), None, Vec::new(), wake.0.clone());
        let child = commands
            .spawn((
                Operation::bundle("dx.tui.cockpit").unwrap().running(),
                JobRequest(task),
                flight,
                job,
                ChildOf(entity),
            ))
            .id();
        commands.entity(entity).insert(CockpitJob(child));
        wake.0.wake_by_ref();
    }
}

fn cancel_cockpit(
    requests: Query<(Entity, &CockpitJob), Without<CliResult>>,
    mut jobs: Query<(&Flight, &mut OperationState), With<RunCockpit>>,
    mut commands: Commands,
) {
    let Some(signal) = Signals::pending() else {
        return;
    };
    for (entity, handle) in &requests {
        let Ok((flight, mut state)) = jobs.get_mut(handle.0) else {
            continue;
        };
        *state = OperationState::Canceled;
        if !flight.pending() {
            commands.entity(handle.0).despawn();
            commands
                .entity(entity)
                .remove::<CockpitJob>()
                .insert(CliResult(Ok((128 + signal) as u8)));
        }
    }
}

fn finish_cockpit(
    results: FinishedCockpits,
    mut requests: Query<
        (&mut CliInvocation, &CockpitJob, Option<&mut HudSession>),
        Without<CliResult>,
    >,
    mut mode: ResMut<TerminalMode>,
    mut hud: ResMut<Hud>,
    mut commands: Commands,
) {
    for (job_entity, result, owner, parent) in &results {
        let root = parent.parent();
        let Ok((mut invocation, handle, session)) = requests.get_mut(root) else {
            commands.entity(job_entity).despawn();
            continue;
        };
        if handle.0 != job_entity || !owner.accepts(Some(parent)) {
            commands.entity(job_entity).despawn();
            continue;
        }
        commands.entity(job_entity).despawn();
        commands.entity(root).remove::<CockpitJob>();
        let outcome = match &result.0 {
            Ok(report) => match &report.result {
                Ok(outcome) => outcome.clone(),
                Err(error) => {
                    let message = error.to_string();
                    if !message.is_empty() {
                        commands.entity(root).insert(FailedDiagnostic(message));
                    }
                    commands
                        .entity(root)
                        .insert(CliResult(Ok(error.code() as u8)));
                    continue;
                }
            },
            Err(error) => {
                commands.entity(root).insert(CliResult(Err(error.clone())));
                continue;
            }
        };
        mode.theme = outcome.theme;
        let Some(command_id) = outcome.action.command_id() else {
            if mode.interactive {
                let theme = mode.theme();
                hud.writer.write_block(&format!(
                    "  {}  {}\n",
                    theme.accent("✦"),
                    theme.muted("See you next build.")
                ));
            }
            commands.entity(root).insert(CliResult::default());
            continue;
        };
        invocation.command = command_id.into();
        if let Some(root_path) = outcome.root {
            invocation
                .arguments
                .insert("root".into(), vec![root_path.into_os_string()]);
        }
        if let Some(branch) = outcome.branch {
            invocation
                .arguments
                .insert("branch".into(), vec![OsString::from(branch)]);
        } else {
            invocation.arguments.remove("branch");
        }
        if outcome.action.uses_profile() {
            invocation.arguments.insert(
                "profile".into(),
                vec![OsString::from(outcome.profile.as_str())],
            );
        }
        if outcome.action == CockpitAction::Desktop {
            invocation.command = "dx.run".into();
        }
        if let Some(mut session) = session {
            session.retarget(&invocation);
        }
    }
}

fn prompt_clean(
    mut previews: MessageReader<CleanPreviewed>,
    mode: Res<TerminalMode>,
    existing: Query<(), With<CleanConfirming>>,
    mut hud: ResMut<Hud>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for preview in previews.read() {
        if !mode.interactive || preview.reclaimable_count == 0 || existing.contains(preview.root) {
            continue;
        }
        hud.writer.stop_ticker();
        hud.writer.clear_transient();
        let job = PromptConfirm {
            reclaimable_count: preview.reclaimable_count,
            free_bytes: preview.free_bytes,
            cache_bytes: preview.cache_bytes,
            color: mode.color,
            width: mode.width,
            theme: mode.theme,
        };
        let (task, flight) = Task::new(job.clone(), None, Vec::new(), wake.0.clone());
        let child = commands
            .spawn((
                Operation::bundle("dx.tui.prompt").unwrap().running(),
                JobRequest(task),
                flight,
                job,
                ChildOf(preview.root),
            ))
            .id();
        commands
            .entity(preview.root)
            .insert((CleanConfirming(preview.targets.clone()), PromptJob(child)));
        wake.0.wake_by_ref();
    }
}

fn cancel_prompt(
    requests: Query<(Entity, &PromptJob), With<CleanConfirming>>,
    mut jobs: Query<(&Flight, &mut OperationState), With<PromptConfirm>>,
    mut commands: Commands,
) {
    if Signals::pending().is_none() {
        return;
    }
    for (entity, handle) in &requests {
        let Ok((flight, mut state)) = jobs.get_mut(handle.0) else {
            continue;
        };
        *state = OperationState::Canceled;
        if !flight.pending() {
            commands.entity(handle.0).despawn();
            commands
                .entity(entity)
                .remove::<(CleanConfirming, PromptJob)>();
        }
    }
}

fn finish_prompt(
    results: FinishedPrompts,
    mut roots: Query<(&mut Tail, &PromptJob, &CleanConfirming)>,
    mut hud: ResMut<Hud>,
    mode: Res<TerminalMode>,
    mut commands: Commands,
) {
    for (job_entity, result, owner, parent) in &results {
        let root = parent.parent();
        let Ok((mut tail, handle, confirming)) = roots.get_mut(root) else {
            commands.entity(job_entity).despawn();
            continue;
        };
        if handle.0 != job_entity || !owner.accepts(Some(parent)) {
            commands.entity(job_entity).despawn();
            continue;
        }
        let targets = confirming.0.clone();
        commands.entity(job_entity).despawn();
        commands
            .entity(root)
            .remove::<(CleanConfirming, PromptJob)>();
        match &result.0 {
            Ok(report) => match &report.result {
                Ok(ConfirmOutcome(true)) => {
                    let mut step = commands.spawn((
                        Step,
                        Name::new("Clean build cache"),
                        StorageCleanStep {
                            apply: true,
                            allowed_targets: Some(targets),
                        },
                        OperationState::Pending,
                        CreatedAt::now(),
                        ChildOf(root),
                        crate::Attempt(1),
                    ));
                    if let Some(previous) = tail.0 {
                        step.insert(Predecessor(previous));
                    }
                    tail.0 = Some(step.id());
                }
                Ok(ConfirmOutcome(false)) => {
                    if mode.interactive {
                        let line = mode.theme().notice_line(
                            NoticeStyle::Detail,
                            "Kept preview only; no caches deleted.",
                        );
                        hud.writer.write_block(&line);
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    if !message.is_empty() {
                        commands.entity(root).insert(FailedDiagnostic(message));
                    }
                    commands
                        .entity(root)
                        .insert(CliResult(Ok(error.code() as u8)));
                }
            },
            Err(error) => {
                commands.entity(root).insert(CliResult(Err(error.clone())));
            }
        }
    }
}

fn track_sessions(
    mut candidates: SessionCandidates,
    mode: Res<TerminalMode>,
    mut hud: ResMut<Hud>,
    mut commands: Commands,
) {
    for (entity, invocation, dispatch, checkout, settings, accepted, session) in &mut candidates {
        match session {
            None => {
                let created = HudSession::from_invocation(invocation, dispatch.is_some());
                commands.entity(entity).insert(created);
            }
            Some(mut session) => {
                if session.branch.is_none()
                    && let Some(checkout) = checkout
                    && let Some(name) = checkout.root.file_name().and_then(|v| v.to_str())
                {
                    session.branch = Some(name.to_owned());
                }
                if let Some(settings) = settings
                    && let Some(profile) = settings
                        .0
                        .get("VMUX_BUILD_PROFILE")
                        .and_then(|value| value.to_str())
                {
                    session.profile = Some(profile.to_owned());
                }
                if mode.interactive && accepted && !session.silent && !session.header_shown {
                    let card = mode.theme().header_card(
                        &session.command,
                        session.branch.as_deref(),
                        session.profile.as_deref(),
                    );
                    hud.writer.write_block(&card);
                    session.header_shown = true;
                }
            }
        }
    }
}

fn observe_artifacts(
    mut events: MessageReader<ArtifactObserved>,
    mut sessions: Query<&mut HudSession>,
) {
    for event in events.read() {
        if let Ok(mut session) = sessions.get_mut(event.root) {
            session.artifact = Some(event.path.clone());
            if event.commit.is_some() {
                session.commit = event.commit.clone();
            }
        }
    }
}

fn present_storage(
    mut reports: MessageReader<StorageReported>,
    mut sessions: Query<&mut HudSession>,
    preflight_steps: Query<&ChildOf, With<crate::steps::StoragePreflightStep>>,
    mode: Res<TerminalMode>,
    mut hud: ResMut<Hud>,
) {
    for report in reports.read() {
        let free_label = format!("{:.1} GiB free", report.free_bytes as f64 / GIB as f64);
        let has_preflight = preflight_steps
            .iter()
            .any(|parent| parent.parent() == report.root);
        let mut defer_check = false;
        let mut duplicate = false;
        let mut silent = false;
        if let Ok(mut session) = sessions.get_mut(report.root) {
            session.free_bytes = Some(report.free_bytes);
            silent = session.silent;
            match report.phase {
                StoragePhase::Check => {
                    if has_preflight {
                        session.pending_check_storage =
                            Some(format!("Build storage: {free_label}\n"));
                        defer_check = true;
                    } else {
                        duplicate = session.preflight_storage_reported;
                        session.preflight_storage_reported = true;
                    }
                }
                StoragePhase::Preflight => {
                    session.pending_check_storage = None;
                    duplicate = session.preflight_storage_reported;
                    session.preflight_storage_reported = true;
                }
                StoragePhase::CleanApplied => {
                    session.clean_applied = true;
                }
                _ => {}
            }
            session.last_step_detail = Some(match report.cache_bytes {
                Some(cache) => format!("{free_label}, {:.1} GiB caches", cache as f64 / GIB as f64),
                None => free_label.clone(),
            });
        }
        if mode.interactive && !silent {
            if let Some(warning) = &report.warning {
                let line = mode.theme().notice_line(NoticeStyle::Warning, warning);
                hud.writer.write_block(&line);
            }
            if matches!(
                report.phase,
                StoragePhase::Report | StoragePhase::CleanPreview | StoragePhase::CleanApplied
            ) && let Some(cache) = report.cache_bytes
            {
                let theme = mode.theme();
                let gauge = theme.storage_gauge(report.free_bytes, cache, 16);
                let text = format!(
                    "  {gauge}  {} free · {} compile caches\n",
                    theme.green(&format!("{:.1} GiB", report.free_bytes as f64 / GIB as f64)),
                    theme.gold(&format!("{:.1} GiB", cache as f64 / GIB as f64)),
                );
                hud.writer.write_block(&text);
            }
            continue;
        }
        match report.phase {
            StoragePhase::Check => {
                if !defer_check && !duplicate {
                    hud.writer
                        .write_stdout(&format!("Build storage: {free_label}\n"));
                }
            }
            StoragePhase::Preflight => {
                if !duplicate {
                    hud.writer
                        .write_stdout(&format!("Build storage: {free_label}\n"));
                }
            }
            StoragePhase::Finish => {
                let profile = report.profile.as_deref().unwrap_or("dev");
                hud.writer.write_stdout(&format!(
                    "Build storage: {free_label} after {profile} build\n"
                ));
            }
            StoragePhase::Report | StoragePhase::CleanPreview | StoragePhase::CleanApplied => {
                let cache = report.cache_bytes.unwrap_or(0) as f64 / GIB as f64;
                hud.writer.write_stdout(&format!(
                    "Build storage: {free_label}, {cache:.1} GiB of compile caches\n"
                ));
            }
        }
        if let Some(warning) = &report.warning {
            hud.writer.write_stderr(&format!("{warning}\n"));
        }
    }
}

fn present_notices(
    mut notices: MessageReader<Notice>,
    sessions: Query<&HudSession>,
    mode: Res<TerminalMode>,
    mut hud: ResMut<Hud>,
) {
    for notice in notices.read() {
        let silent = notice
            .root
            .and_then(|root| sessions.get(root).ok())
            .is_some_and(|session| session.silent);
        if mode.interactive && !silent {
            let style = match notice.tone {
                NoticeTone::Info => NoticeStyle::Info,
                NoticeTone::Warning => NoticeStyle::Warning,
                NoticeTone::Stdout => NoticeStyle::Detail,
            };
            let line = mode.theme().notice_line(style, &notice.text);
            hud.writer.write_block(&line);
            continue;
        }
        match notice.tone {
            NoticeTone::Stdout => {
                hud.writer.write_stdout(&format!("{}\n", notice.text));
            }
            NoticeTone::Info | NoticeTone::Warning => {
                hud.writer.write_stderr(&format!("{}\n", notice.text));
            }
        }
    }
}

#[derive(SystemParam)]
struct StepCounter<'w, 's> {
    all: Query<'w, 's, &'static ChildOf, With<Step>>,
    done: Query<'w, 's, &'static ChildOf, (With<Step>, With<Completed>)>,
}

impl StepCounter<'_, '_> {
    fn counts(&self, root: Entity) -> (usize, usize) {
        let mut total = 0usize;
        for parent in &self.all {
            if parent.parent() == root {
                total += 1;
            }
        }
        let mut done = 0usize;
        for parent in &self.done {
            if parent.parent() == root {
                done += 1;
            }
        }
        (done, total)
    }
}

fn track_steps(
    mut steps: RunningSteps,
    counter: StepCounter,
    mut sessions: Query<&mut HudSession>,
    mode: Res<TerminalMode>,
    mut hud: ResMut<Hud>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    if !mode.interactive {
        return;
    }
    for (entity, name, parent, passthrough, running, completed, state, timing) in &mut steps {
        let Ok(mut session) = sessions.get_mut(parent.parent()) else {
            continue;
        };
        if session.silent {
            continue;
        }
        if running && timing.is_none() {
            let started = Instant::now();
            if passthrough {
                hud.writer.stop_ticker();
                hud.writer.clear_transient();
                let (done, total) = counter.counts(parent.parent());
                let banner = mode.theme().step_banner(done, total, name.as_str());
                hud.writer.write_block(&banner);
                hud.active_step = Some(entity);
                hud.active_passthrough = true;
            } else {
                hud.active_step = Some(entity);
                hud.active_passthrough = false;
                hud.writer.start_ticker(wake.0.clone());
            }
            commands.entity(entity).insert(StepTiming {
                started,
                announced: true,
                reported_done: false,
            });
            continue;
        }
        if completed
            && let Some(mut timing) = timing
            && timing.announced
            && !timing.reported_done
        {
            timing.reported_done = true;
            let elapsed = timing.started.elapsed();
            if hud.active_step == Some(entity) {
                hud.active_step = None;
                hud.active_passthrough = false;
                hud.writer.stop_ticker();
            }
            let detail = session.last_step_detail.take();
            let line = if *state == OperationState::Failed {
                mode.theme().step_fail_line(name.as_str(), elapsed)
            } else {
                mode.theme()
                    .step_done_line(name.as_str(), detail.as_deref(), elapsed)
            };
            hud.writer.write_block(&line);
        }
    }
}

type ActiveSteps<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Name, &'static ChildOf, &'static StepTiming),
    (With<Step>, With<Running>, Without<PassthroughStep>),
>;

fn animate(
    steps: ActiveSteps,
    counter: StepCounter,
    bootstraps: Query<&crate::tooling::Compiling>,
    sessions: Query<&HudSession>,
    mode: Res<TerminalMode>,
    mut hud: ResMut<Hud>,
    wake: Res<JobWake>,
) {
    if !mode.interactive || hud.active_passthrough {
        return;
    }
    if hud.last_tick.elapsed() >= Duration::from_millis(80) {
        hud.frame = hud.frame.wrapping_add(1);
        hud.last_tick = Instant::now();
    }
    if let Some(compiling) = bootstraps.iter().next() {
        hud.writer.start_ticker(wake.0.clone());
        let line = mode.theme().active_line(
            hud.frame,
            0,
            0,
            "Updating development CLI",
            compiling.started.elapsed(),
            None,
        );
        hud.writer.draw_transient(&line);
        return;
    }
    let Some((_, name, parent, timing)) = steps.iter().next() else {
        hud.writer.stop_ticker();
        hud.writer.clear_transient();
        return;
    };
    let Ok(session) = sessions.get(parent.parent()) else {
        return;
    };
    if session.silent {
        return;
    }
    let (done, total) = counter.counts(parent.parent());
    let line = mode.theme().active_line(
        hud.frame,
        done,
        total,
        name.as_str(),
        timing.started.elapsed(),
        session.last_step_detail.as_deref(),
    );
    hud.writer.draw_transient(&line);
}

fn present_results(
    mut results: ResultOperations,
    artifacts: Query<&Artifact, With<vmux_ecs::Ready>>,
    mode: Res<TerminalMode>,
    mut hud: ResMut<Hud>,
    mut commands: Commands,
) {
    for (entity, mut result, session, diagnostic, children) in &mut results {
        hud.writer.stop_ticker();
        hud.writer.clear_transient();
        commands.entity(entity).insert(ResultPresented);
        let silent = session.is_some_and(|session| session.silent);
        let mut ready_artifacts = Vec::new();
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(artifact) = artifacts.get(child) {
                    ready_artifacts.push(artifact.0.clone());
                }
            }
        }
        if !mode.interactive || silent {
            if let Some(session) = session
                && let Some(line) = &session.pending_check_storage
            {
                hud.writer.write_stdout(line);
            }
            match &result.0 {
                Ok(0) => {
                    for path in &ready_artifacts {
                        hud.writer
                            .write_stdout(&format!("Ready artifact: {}\n", path.display()));
                    }
                }
                Ok(_) => {
                    if let Some(diagnostic) = diagnostic {
                        hud.writer
                            .write_stderr(&format!("vmux: {}\n", diagnostic.0));
                    }
                }
                Err(_) => {}
            }
            continue;
        }
        let theme = mode.theme();
        match &result.0 {
            Ok(0) => {
                if let Some(session) = session
                    && session.header_shown
                {
                    let artifact = ready_artifacts
                        .last()
                        .or(session.artifact.as_ref())
                        .map(PathBuf::as_path);
                    let summary_command =
                        if session.command == "cache clean" && !session.clean_applied {
                            "cache clean preview"
                        } else {
                            session.command.as_str()
                        };
                    let card = theme.summary_card(
                        summary_command,
                        (session.branch.as_deref(), session.commit.as_deref()),
                        session.profile.as_deref(),
                        artifact,
                        session.free_bytes,
                        session.started.elapsed(),
                    );
                    hud.writer.write_block(&card);
                }
            }
            Ok(_) => {
                if let Some(diagnostic) = diagnostic {
                    let hint = RecoveryHint::for_error(&diagnostic.0);
                    let card = theme.error_card(&diagnostic.0, hint);
                    hud.writer.write_block(&card);
                }
            }
            Err(error) => {
                let hint = RecoveryHint::for_error(error);
                let card = theme.error_card(error, hint);
                hud.writer.write_block(&card);
                result.0 = Ok(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_storage_check_and_preflight_deduplicate_per_operation() {
        let mut app = App::new();
        app.insert_resource(JobWake(
            std::sync::Arc::new(vmux_ecs::cli::CliWake::current()).into(),
        ))
        .add_plugins(TuiPlugin)
        .insert_resource(TerminalMode {
            interactive: false,
            color: false,
            width: 80,
            theme: ThemePreset::CatppuccinMocha,
        });
        let root = app
            .world_mut()
            .spawn((
                CliInvocation {
                    command: "dx.build".into(),
                    arguments: Default::default(),
                },
                HudSession::from_invocation(
                    &CliInvocation {
                        command: "dx.build".into(),
                        arguments: Default::default(),
                    },
                    false,
                ),
            ))
            .id();
        app.world_mut()
            .spawn((crate::steps::StoragePreflightStep, ChildOf(root)));
        app.world_mut().write_message(StorageReported {
            root,
            free_bytes: 400 * GIB,
            cache_bytes: None,
            warning: None,
            phase: StoragePhase::Check,
            profile: None,
        });
        app.update();
        let session = app.world().get::<HudSession>(root).unwrap();
        assert!(session.pending_check_storage.is_some());
        assert!(!session.preflight_storage_reported);
        app.world_mut().write_message(StorageReported {
            root,
            free_bytes: 399 * GIB,
            cache_bytes: None,
            warning: None,
            phase: StoragePhase::Preflight,
            profile: None,
        });
        app.update();
        let session = app.world().get::<HudSession>(root).unwrap();
        assert!(session.pending_check_storage.is_none());
        assert!(session.preflight_storage_reported);
        assert_eq!(session.free_bytes, Some(399 * GIB));
    }

    #[cfg(not(feature = "runtime-cli"))]
    #[test]
    fn non_interactive_tui_command_fails_fast_without_spawning_a_cockpit() {
        let mut app = App::new();
        app.insert_resource(JobWake(
            std::sync::Arc::new(vmux_ecs::cli::CliWake::current()).into(),
        ))
        .add_plugins(TuiPlugin)
        .insert_resource(TerminalMode {
            interactive: false,
            color: false,
            width: 80,
            theme: ThemePreset::CatppuccinMocha,
        });
        let root = app
            .world_mut()
            .spawn(CliInvocation {
                command: "dx.tui".into(),
                arguments: Default::default(),
            })
            .id();
        app.update();
        let result = app.world().get::<CliResult>(root).unwrap();
        assert!(
            result
                .0
                .as_ref()
                .unwrap_err()
                .contains("Interactive TUI requires a terminal")
        );
    }
}
