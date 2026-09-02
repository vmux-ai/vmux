use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use vmux_command::host::FileStatusPicked;
use vmux_command::{
    AppCommand, BrowserBarCommand, BrowserCommand, CommandIssued, CommandIssuer, ReadAppCommands,
};
use vmux_core::event::{
    ExplorerGoto, FILE_KEY_EVENT, FileEncoding, FileEncodingAction, FileEncodingSet, FileIndent,
    FileKey, FileLineEnding, FileShapeSet, FileStatusPickerOpen,
};
use vmux_wire::command_bar::CommandBarPick;

use crate::host::plugin::{EditState, FileView};
use crate::host::shape::BufferShape;

pub(crate) struct FileKeyPlugin;

impl Plugin for FileKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(FileStatusPickerOpen,)>::default())
            .add_systems(Update, echo_key_command.in_set(ReadAppCommands))
            .add_systems(Update, apply_status_picks)
            .add_observer(open_status_picker);
    }
}

fn echo_key_command(mut issued: MessageReader<CommandIssued>, mut commands: Commands) {
    for issue in issued.read() {
        let AppCommand::File(key) = issue.command else {
            continue;
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            issue.caller,
            FILE_KEY_EVENT,
            &FileKey::from(key),
        ));
    }
}

fn open_status_picker(
    trigger: On<BinReceive<FileStatusPickerOpen>>,
    views: Query<(), With<FileView>>,
    mut issuer: CommandIssuer,
) {
    let caller = trigger.event().webview;
    if !views.contains(caller) {
        return;
    }
    let Some(bar) = BrowserBarCommand::opening(trigger.event().payload.picker) else {
        return;
    };
    issuer.issue(caller, AppCommand::Browser(BrowserCommand::Bar(bar)));
}

fn apply_status_picks(
    mut picked: MessageReader<FileStatusPicked>,
    children: Query<&Children>,
    editors: Query<(), With<FileView>>,
    shapes: Query<&EditState>,
    mut commands: Commands,
) {
    for message in picked.read() {
        let Some(stack) = message.stack else {
            continue;
        };
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(entity) = kids.iter().find(|child| editors.contains(*child)) else {
            continue;
        };
        match &message.pick {
            CommandBarPick::Picker(_) => {}
            CommandBarPick::GotoLine { line } => {
                commands.trigger(BinReceive {
                    webview: entity,
                    payload: ExplorerGoto {
                        path: String::new(),
                        line: *line,
                    },
                });
            }
            CommandBarPick::Indent { spaces, width } => {
                let Ok(edit) = shapes.get(entity) else {
                    continue;
                };
                let shape = BufferShape::of(&edit.core.buffer.rope);
                commands.trigger(BinReceive {
                    webview: entity,
                    payload: FileShapeSet {
                        indent: FileIndent {
                            spaces: *spaces,
                            width: *width,
                        },
                        line_ending: shape.line_ending,
                    },
                });
            }
            CommandBarPick::LineEnding { crlf } => {
                let Ok(edit) = shapes.get(entity) else {
                    continue;
                };
                let shape = BufferShape::of(&edit.core.buffer.rope);
                let line_ending = match crlf {
                    true => FileLineEnding::Crlf,
                    false => FileLineEnding::Lf,
                };
                commands.trigger(BinReceive {
                    webview: entity,
                    payload: FileShapeSet {
                        indent: shape.indent,
                        line_ending,
                    },
                });
            }
            CommandBarPick::Encoding { label, save } => {
                let Some(encoding) = FileEncoding::of_label(label) else {
                    continue;
                };
                let action = match save {
                    true => FileEncodingAction::Save,
                    false => FileEncodingAction::Reopen,
                };
                commands.trigger(BinReceive {
                    webview: entity,
                    payload: FileEncodingSet { encoding, action },
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::FileKeyCommand;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<FileKey, rkyv::rancor::Error>(&trigger.payload)
                .map(|key| format!("{key:?}"))
                .unwrap_or_else(|_| "undecodable".to_string());
            echoed
                .0
                .push((trigger.webview, format!("{}:{decoded}", trigger.id)));
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(FileKeyPlugin)
                .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
                .add_message::<CommandIssued>()
                .add_message::<FileStatusPicked>()
                .init_resource::<Echoed>()
                .add_observer(Echoed::record);
            app
        }

        fn issue(app: &mut App, caller: Entity, command: AppCommand) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<CommandIssued>>()
                .write(CommandIssued { caller, command });
            app.update();
        }
    }

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            pressed,
            AppCommand::File(FileKeyCommand::PanelChoose),
        );

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{FILE_KEY_EVENT}:PanelChoose"))]
        );
        assert!(
            !app.world()
                .resource::<Echoed>()
                .0
                .iter()
                .any(|(entity, _)| *entity == other)
        );
    }

    #[test]
    fn a_command_that_is_not_a_file_key_is_left_alone() {
        let mut app = Echo::app();
        let caller = app.world_mut().spawn_empty().id();

        Echo::issue(
            &mut app,
            caller,
            AppCommand::Terminal(vmux_command::TerminalCommand::Clear),
        );

        assert!(app.world().resource::<Echoed>().0.is_empty());
    }

    #[derive(Resource, Default)]
    struct Reopened(Vec<(Entity, FileEncoding)>);

    impl Reopened {
        fn record(trigger: On<BinReceive<FileEncodingSet>>, mut seen: ResMut<Self>) {
            if trigger.event().payload.action != FileEncodingAction::Reopen {
                return;
            }
            seen.0
                .push((trigger.event().webview, trigger.event().payload.encoding));
        }
    }

    struct Picks;

    impl Picks {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(FileKeyPlugin)
                .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
                .add_message::<CommandIssued>()
                .add_message::<FileStatusPicked>()
                .init_resource::<Reopened>()
                .add_observer(Reopened::record);
            app
        }

        fn stack_with_editor(app: &mut App) -> (Entity, Entity) {
            let editor = app
                .world_mut()
                .spawn(FileView {
                    path: std::path::PathBuf::from("/tmp/a.txt"),
                })
                .id();
            let stack = app.world_mut().spawn(children![]).add_child(editor).id();
            (stack, editor)
        }

        fn submit(app: &mut App, stack: Option<Entity>, pick: CommandBarPick) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<FileStatusPicked>>()
                .write(FileStatusPicked { stack, pick });
            app.update();
        }
    }

    #[test]
    fn an_encoding_pick_reaches_the_editor_under_the_focused_stack() {
        let mut app = Picks::app();
        let (stack, editor) = Picks::stack_with_editor(&mut app);

        Picks::submit(
            &mut app,
            Some(stack),
            CommandBarPick::Encoding {
                label: "Shift_JIS".to_string(),
                save: false,
            },
        );

        assert_eq!(
            app.world().resource::<Reopened>().0,
            vec![(editor, FileEncoding::ShiftJis)],
            "the stack itself must not be asked to reopen"
        );
    }

    #[test]
    fn a_pick_with_no_focused_editor_asks_nothing_to_reopen() {
        let mut app = Picks::app();
        let empty = app.world_mut().spawn(children![]).id();

        for stack in [None, Some(empty)] {
            Picks::submit(
                &mut app,
                stack,
                CommandBarPick::Encoding {
                    label: "Shift_JIS".to_string(),
                    save: false,
                },
            );
        }

        assert!(app.world().resource::<Reopened>().0.is_empty());
    }

    #[test]
    fn an_unknown_encoding_label_is_refused_rather_than_guessed() {
        let mut app = Picks::app();
        let (stack, _) = Picks::stack_with_editor(&mut app);

        Picks::submit(
            &mut app,
            Some(stack),
            CommandBarPick::Encoding {
                label: "Klingon".to_string(),
                save: false,
            },
        );

        assert!(app.world().resource::<Reopened>().0.is_empty());
    }
}
