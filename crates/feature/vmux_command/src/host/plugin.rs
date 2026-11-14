use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::UiInput;
use vmux_api::input::KeyStroke;
use vmux_api::protocol::AgentCommandResult;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
    CommandOrigin,
};

use super::tool::AgentInvokeCommand;
use crate::host::definition::{
    CommandDefinition, CommandInvocation, CommandRuntimePlugin, DispatchCommandInvocations,
    WriteCommandRequests,
};
use crate::host::shortcut_driver::{KeyCombo, KeyContext, Keymap};
use crate::host::snapshot::UiStatePlugin;
use vmux_ecs::team::{Agent, Profile, User};

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        if !app.is_plugin_added::<vmux_ecs::wake::WakePlugin>() {
            app.add_plugins(vmux_ecs::wake::WakePlugin);
        }
        app.init_resource::<Settle>();
        app.add_agent_request::<AgentInvokeCommand>()
            .add_plugins((UiStatePlugin, crate::CommandToolPlugin))
            .add_observer(resolve)
            .add_systems(Update, invoke.after(AgentRequestRouteSet))
            .add_systems(
                Update,
                log.after(WriteCommandRequests)
                    .before(DispatchCommandInvocations),
            )
            .add_systems(Last, wake);
    }
}

fn resolve(
    trigger: On<UiInput<KeyStroke>>,
    keymaps: Query<&Keymap>,
    contexts: Query<&KeyContext>,
    mut invocations: MessageWriter<CommandInvocation>,
) {
    let page = trigger.event_target();
    let (Ok(keymap), Ok(context), Some(pressed)) = (
        keymaps.single(),
        contexts.get(page),
        KeyCombo::from_stroke(&trigger.payload),
    ) else {
        return;
    };
    let Some(command) = keymap.in_context(context).scoped(&pressed) else {
        return;
    };
    invocations.write(CommandInvocation::new(page, command));
}

fn invoke(
    mut requests: MessageReader<AgentRequestMessage<AgentInvokeCommand>>,
    definitions: Query<&CommandDefinition>,
    mut invocations: MessageWriter<CommandInvocation>,
    agents: Query<(Entity, &Agent, Option<&vmux_ecs::ProcessId>)>,
    user: Query<Entity, With<User>>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let args = match vmux_ecs::JsonArguments::try_from(&request.payload.args) {
            Ok(args) => args.0,
            Err(message) => {
                responses.write(request.reply.response(AgentCommandResult::Error(message)));
                continue;
            }
        };
        let caller = match &request.origin {
            CommandOrigin::Agent {
                anchor: Some(pid), ..
            } => agents
                .iter()
                .find(|(_, _, process)| process.as_ref().is_some_and(|process| *process == pid))
                .map(|(entity, _, _)| entity),
            CommandOrigin::Agent { sid: Some(sid), .. } if !sid.is_empty() => agents
                .iter()
                .find(|(_, agent, _)| agent.sid.as_str() == sid.as_str())
                .map(|(entity, _, _)| entity),
            CommandOrigin::User => user.single().ok(),
            _ => None,
        }
        .unwrap_or(Entity::PLACEHOLDER);
        let Some(definition) = definitions
            .iter()
            .find(|definition| definition.matches(&request.payload.id))
        else {
            responses.write(request.reply.response(AgentCommandResult::Error(format!(
                "unknown app command: {}",
                request.payload.id
            ))));
            continue;
        };
        let result = if request.origin.is_agent() {
            definition.agent_invocation(caller, args)
        } else {
            definition.user_invocation(caller, args)
        };
        match result {
            Ok(invocation) => {
                invocations.write(invocation);
                responses.write(request.reply.ok());
            }
            Err(message) => {
                responses.write(request.reply.response(AgentCommandResult::Error(message)));
            }
        }
    }
}

const COMMAND_SETTLE_WINDOW: std::time::Duration = std::time::Duration::from_millis(150);
const SETTLE_FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

#[derive(Resource, Default)]
struct Settle {
    until: Option<std::time::Instant>,
    timer: Option<Task<()>>,
}

impl Settle {
    fn begin(&mut self, now: std::time::Instant) {
        self.until = Some(now + COMMAND_SETTLE_WINDOW);
    }

    fn schedule(&mut self, now: std::time::Instant, wake: std::task::Waker) {
        let Some(until) = self.until else {
            return;
        };
        let deadline = (now + SETTLE_FRAME_INTERVAL).min(until);
        self.timer = Some(IoTaskPool::get().spawn(async move {
            std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
            wake.wake();
        }));
    }
}

fn wake(
    mut settle: ResMut<Settle>,
    mut reader: MessageReader<CommandInvocation>,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    let now = std::time::Instant::now();
    if reader.read().count() > 0 {
        settle.begin(now);
    }
    let Some(until) = settle.until else {
        return;
    };
    if now >= until {
        settle.until = None;
        settle.timer = None;
        return;
    }
    let Some(wake) = wake else {
        settle.until = None;
        settle.timer = None;
        return;
    };
    if let Some(timer) = &mut settle.timer {
        if future::block_on(future::poll_once(timer)).is_none() {
            return;
        }
        settle.timer = None;
    }
    settle.schedule(now, wake.0.clone());
}

fn log(mut reader: MessageReader<CommandInvocation>, profiles: Query<(&Profile, Has<User>)>) {
    for invocation in reader.read() {
        let who = profiles
            .get(invocation.caller)
            .map(|(p, is_user)| format!("{} ({})", p.name, if is_user { "user" } else { "agent" }))
            .unwrap_or_else(|_| "unknown".to_string());
        info!(
            target: "vmux_command::invocation",
            caller = %who,
            id = %invocation.id,
            "CommandInvocation"
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;
    use bevy::input::keyboard::KeyCode;
    use vmux_api::input::KeyModifiers;

    use super::*;
    use crate::host::shortcut_driver::{Binding, Modifiers, Shortcut, Source, When};

    #[derive(Default)]
    struct SettleWake(std::sync::atomic::AtomicUsize);

    impl SettleWake {
        fn new(app: &mut App) -> std::sync::Arc<Self> {
            let observed = std::sync::Arc::new(Self::default());
            app.insert_resource(vmux_ecs::JobWake(std::task::Waker::from(observed.clone())));
            observed
        }
    }

    impl std::task::Wake for SettleWake {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn command_settlement_wakes_on_paced_timers_and_then_becomes_idle() {
        let mut app = Seam::app();
        let observed = SettleWake::new(&mut app);
        app.world_mut()
            .write_message(CommandInvocation::new(Entity::PLACEHOLDER, "settle-test"));
        app.update();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while observed.0.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "settlement timer never woke"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(observed.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        while app.world().resource::<Settle>().until.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "settlement did not finish"
            );
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        let finished = observed.0.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            finished < 20,
            "settlement requested {finished} unpaced updates"
        );
        for _ in 0..10 {
            app.update();
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(
            observed.0.load(std::sync::atomic::Ordering::SeqCst),
            finished
        );
    }

    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        shift: false,
        alt: false,
        super_key: false,
    };

    struct Seam;

    impl Seam {
        fn app() -> App {
            let mut keymap = Keymap::default();
            keymap.extend(
                Source::Settings,
                [
                    Binding {
                        shortcut: Shortcut::Direct(KeyCombo {
                            key: KeyCode::KeyN,
                            modifiers: CTRL,
                        }),
                        command: "command_bar_next".to_string(),
                        when: When::parse("command-bar"),
                    },
                    Binding {
                        shortcut: Shortcut::Direct(KeyCombo {
                            key: KeyCode::KeyX,
                            modifiers: CTRL,
                        }),
                        command: "stack_close".to_string(),
                        when: None,
                    },
                ],
            );
            keymap.register(["command_bar_next", "stack_close"]);

            let mut app = App::new();
            app.add_plugins((MinimalPlugins, CommandPlugin));
            app.update();
            let mut query = app.world_mut().query::<&mut Keymap>();
            *query.single_mut(app.world_mut()).expect("command keymap") = keymap;
            app
        }

        fn page(app: &mut App, context: &[&str]) -> Entity {
            let context: KeyContext = context.iter().map(|key| (*key).to_string()).collect();
            app.world_mut().spawn(context).id()
        }

        fn press(app: &mut App, page: Entity, code: &str) -> Vec<(Entity, String)> {
            app.world_mut().trigger(UiInput {
                webview: page,
                payload: KeyStroke {
                    key: code.to_string(),
                    code: code.to_string(),
                    mods: KeyModifiers::from(CTRL),
                    text: None,
                    repeat: false,
                },
            });
            app.update();
            app.world_mut()
                .resource_mut::<Messages<CommandInvocation>>()
                .drain()
                .map(|invocation| (invocation.caller, invocation.id))
                .collect()
        }
    }

    #[test]
    fn a_scoped_binding_resolves_only_on_the_surface_that_published_it() {
        let mut app = Seam::app();
        let bar = Seam::page(&mut app, &["command-bar"]);
        let plain = Seam::page(&mut app, &["terminal"]);

        assert_eq!(
            Seam::press(&mut app, bar, "KeyN"),
            vec![(bar, "command_bar_next".to_string())]
        );
        assert_eq!(Seam::press(&mut app, plain, "KeyN"), vec![]);
    }

    #[test]
    fn an_unconditional_binding_is_not_answered_a_second_time() {
        let mut app = Seam::app();
        let bar = Seam::page(&mut app, &["command-bar"]);

        assert_eq!(Seam::press(&mut app, bar, "KeyX"), vec![]);
    }

    #[derive(Resource, Default)]
    struct Answered(Vec<bool>);

    impl Answered {
        fn record(
            trigger: On<UiInput<KeyStroke>>,
            keymap: Single<&Keymap>,
            contexts: Query<&KeyContext>,
            mut answered: ResMut<Self>,
        ) {
            let is_answered = contexts
                .get(trigger.event_target())
                .ok()
                .and_then(|context| {
                    KeyCombo::from_stroke(&trigger.payload).map(|key| (context, key))
                })
                .and_then(|(context, key)| keymap.in_context(context).scoped(&key))
                .is_some();
            answered.0.push(is_answered);
        }
    }

    #[test]
    fn only_a_scoped_binding_takes_a_key_from_the_surfaces_own_keymap() {
        let mut app = Seam::app();
        app.init_resource::<Answered>()
            .add_observer(Answered::record);
        let bar = Seam::page(&mut app, &["command-bar"]);
        let plain = Seam::page(&mut app, &["terminal"]);

        Seam::press(&mut app, bar, "KeyN");
        Seam::press(&mut app, plain, "KeyN");
        Seam::press(&mut app, bar, "KeyX");

        assert_eq!(
            app.world().resource::<Answered>().0,
            vec![true, false, false]
        );
    }
}
