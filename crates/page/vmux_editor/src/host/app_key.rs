use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{CommandBarPick, CommandBarPicker};
use vmux_command::host::FileStatusPicked;
use vmux_command::{
    CommandDefinition, CommandDispatch, CommandInvocation, CommandRuntimePlugin,
    RegisterCommandDefinitions,
};
use vmux_core::event::{
    ExplorerGoto, FileEncoding, FileEncodingAction, FileEncodingSet, FileIndent, FileKey,
    FileLineEnding, FileShapeSet, FileStatusPickerOpen,
};

use crate::host::editor::{Editor, FileView};
use crate::host::shape::BufferShape;

pub(crate) struct KeyPlugin;

impl Plugin for KeyPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins(UiEventPlugin::<(FileStatusPickerOpen,)>::default())
            .add_systems(Startup, spawn_commands.in_set(RegisterCommandDefinitions))
            .add_systems(Update, apply_status_picks)
            .add_observer(echo_key_command)
            .add_observer(open_status_picker);
    }
}

#[derive(Component)]
struct FileKeyBinding(FileKey);

fn spawn_commands(mut commands: Commands) {
    for (definition, key) in [
        (
            CommandDefinition::new("file_toggle_explorer", "Toggle Explorer", "Editor")
                .hidden()
                .direct_when("Super+b", Some("files")),
            FileKey::ToggleExplorer,
        ),
        (
            CommandDefinition::new("file_reveal_in_explorer", "Reveal In Explorer", "Editor")
                .hidden()
                .direct_when("Super+Shift+e", Some("files"))
                .direct_when("Ctrl+Shift+e", Some("files")),
            FileKey::RevealInExplorer,
        ),
        (
            CommandDefinition::new("file_panel_next", "Next Panel Row", "Editor")
                .hidden()
                .direct_when("ArrowDown", Some("files.panel")),
            FileKey::PanelNext,
        ),
        (
            CommandDefinition::new("file_panel_previous", "Previous Panel Row", "Editor")
                .hidden()
                .direct_when("ArrowUp", Some("files.panel")),
            FileKey::PanelPrevious,
        ),
        (
            CommandDefinition::new("file_panel_choose", "Choose Panel Row", "Editor")
                .hidden()
                .direct_when("Enter", Some("files.panel"))
                .direct_when("Tab", Some("files.panel")),
            FileKey::PanelChoose,
        ),
        (
            CommandDefinition::new("file_panel_dismiss", "Close Panel", "Editor")
                .hidden()
                .direct_when("Escape", Some("files.panel")),
            FileKey::PanelDismiss,
        ),
        (
            CommandDefinition::new("file_find", "Find In File", "Editor")
                .direct_when("Super+f", Some("files")),
            FileKey::Find { forward: true },
        ),
        (
            CommandDefinition::new("file_find_in_files", "Find In Files", "Editor")
                .direct_when("Super+Shift+f", Some("files"))
                .direct_when("Ctrl+Shift+f", Some("files")),
            FileKey::FindInFiles,
        ),
    ] {
        commands.spawn((definition, FileKeyBinding(key)));
    }
}

fn echo_key_command(
    trigger: On<CommandDispatch>,
    keys: Query<&FileKeyBinding>,
    mut commands: Commands,
) {
    let Ok(key) = keys.get(trigger.event().command()) else {
        return;
    };
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_core::event::FileUiState>::from_event(
            trigger.event().invocation().caller,
            &key.0,
        ),
    );
}

fn open_status_picker(
    trigger: On<UiInput<FileStatusPickerOpen>>,
    views: Query<(), With<FileView>>,
    mut invocations: MessageWriter<CommandInvocation>,
) {
    let caller = trigger.event().webview;
    if !views.contains(caller) {
        return;
    }
    let id = match trigger.event().payload.picker {
        CommandBarPicker::GotoLine => "browser_open_goto_line",
        CommandBarPicker::Indent => "browser_open_indentation",
        CommandBarPicker::LineEnding => "browser_open_line_ending",
        CommandBarPicker::Encoding => "browser_open_encoding",
        CommandBarPicker::EncodingReopen => "browser_open_reopen_with_encoding",
        CommandBarPicker::EncodingSave => "browser_open_save_with_encoding",
        CommandBarPicker::Space => return,
    };
    invocations.write(CommandInvocation::new(caller, id));
}

fn apply_status_picks(
    mut picked: MessageReader<FileStatusPicked>,
    children: Query<&Children>,
    editors: Query<(), With<FileView>>,
    shapes: Query<&Editor>,
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
                commands.trigger(UiInput {
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
                let shape = BufferShape::detect(&edit.core.buffer.rope);
                commands.trigger(UiInput {
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
                let shape = BufferShape::detect(&edit.core.buffer.rope);
                let line_ending = match crlf {
                    true => FileLineEnding::Crlf,
                    false => FileLineEnding::Lf,
                };
                commands.trigger(UiInput {
                    webview: entity,
                    payload: FileShapeSet {
                        indent: shape.indent,
                        line_ending,
                    },
                });
            }
            CommandBarPick::Encoding { label, save } => {
                let Ok(encoding) = FileEncoding::try_from(label.as_str()) else {
                    continue;
                };
                let action = match save {
                    true => FileEncodingAction::Save,
                    false => FileEncodingAction::Reopen,
                };
                commands.trigger(UiInput {
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
    use vmux_command::CommandInvocation;
    use vmux_core::event::FileUiStatePatch;
    use vmux_core::host::FileUiStateWrite;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, FileKey)>);

    impl Echoed {
        fn record(trigger: On<FileUiStateWrite>, mut echoed: ResMut<Self>) {
            let FileUiStatePatch::Key(key) = trigger.event().patch() else {
                return;
            };
            echoed.0.push((trigger.event().webview(), *key));
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(KeyPlugin)
                .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
                .add_message::<FileStatusPicked>()
                .init_resource::<Echoed>()
                .add_observer(Echoed::record);
            app
        }

        fn issue(app: &mut App, caller: Entity, id: &str) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<CommandInvocation>>()
                .write(CommandInvocation::new(caller, id));
            app.update();
        }
    }

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(&mut app, pressed, "file_panel_choose");

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, FileKey::PanelChoose)]
        );
        assert!(
            !app.world()
                .resource::<Echoed>()
                .0
                .iter()
                .any(|(entity, _)| *entity == other)
        );
    }

    #[derive(Resource, Default)]
    struct Reopened(Vec<(Entity, FileEncoding)>);

    impl Reopened {
        fn record(trigger: On<UiInput<FileEncodingSet>>, mut seen: ResMut<Self>) {
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
                .add_plugins(KeyPlugin)
                .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
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
