use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{CommandBarPick, CommandBarPicker, PickRequest};
use vmux_command::{
    CommandBarDismiss, CommandBarOpenRequest, ResolvedLocale, WriteCommandBarRequests,
};
use vmux_command::{CommandDispatch, CommandInvocation, CommandRuntimePlugin};
use vmux_ecs::event::{
    ExplorerGoto, FileEncoding, FileEncodingReopenRequest, FileEncodingSaveRequest, FileIndent,
    FileLineEnding, FileShapeSet, FileStatusPickerOpen,
};
#[cfg(test)]
use vmux_ecs::manifest::FeaturePlugin;

use crate::host::editing::FileFindOpenRequest;
use crate::host::editor::{Editor, FileView};
use crate::host::explorer::{
    ExplorerFindInFilesRequest, ExplorerRevealRequest, ExplorerToggleRequest,
};
use crate::host::panel::{
    FilePanelChooseRequest, FilePanelDismissRequest, FilePanelNextRequest, FilePanelPreviousRequest,
};
use crate::host::picker_driver::EditorPicks;
use crate::host::shape::BufferShape;
use crate::picker::EditorPicker;
use vmux_ui::i18n::Locale;

pub(crate) struct KeyPlugin;

impl Plugin for KeyPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins(UiEventPlugin::<(FileStatusPickerOpen, PickRequest)>::default())
            .add_message::<CommandBarOpenRequest>()
            .add_message::<FileStatusPicked>()
            .add_message::<OpenStatusPickerRequest>()
            .add_systems(
                Update,
                open_bound_status_picker.in_set(WriteCommandBarRequests),
            )
            .add_systems(Update, apply_status_picks)
            .add_observer(toggle_explorer)
            .add_observer(reveal_in_explorer)
            .add_observer(open_find)
            .add_observer(open_find_in_files)
            .add_observer(dispatch_panel_next_command)
            .add_observer(dispatch_panel_previous_command)
            .add_observer(dispatch_panel_choose_command)
            .add_observer(dispatch_panel_dismiss_command)
            .add_observer(open_status_picker)
            .add_observer(pick_status);
    }
}

#[vmux_command::command]
struct FileToggleExplorerBinding;

#[vmux_command::command]
struct FileRevealInExplorerBinding;

#[vmux_command::command]
struct FileFindBinding;

#[vmux_command::command]
struct FileFindInFilesBinding;

#[vmux_command::command]
struct FilePanelNextBinding;

#[vmux_command::command]
struct FilePanelPreviousBinding;

#[vmux_command::command]
struct FilePanelChooseBinding;

#[vmux_command::command]
struct FilePanelDismissBinding;

#[vmux_command::command(message)]
#[derive(Message, Clone, Debug, PartialEq, Eq)]
struct OpenStatusPickerRequest(CommandBarPicker);

impl TryFrom<&CommandInvocation> for OpenStatusPickerRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        EditorPicker::from_command(&invocation.id)
            .map(Self)
            .ok_or(())
    }
}

#[derive(Message, Clone)]
struct FileStatusPicked {
    stack: Option<Entity>,
    pick: CommandBarPick,
}

fn open_bound_status_picker(
    mut requests: MessageReader<OpenStatusPickerRequest>,
    locale: Option<Res<ResolvedLocale>>,
    mut open: MessageWriter<CommandBarOpenRequest>,
) {
    if let Some(request) = requests.read().last() {
        let locale = locale
            .as_deref()
            .map(|locale| locale.0.clone())
            .unwrap_or_else(Locale::preferred);
        open.write(EditorPicks::request(request.0.clone(), &locale));
    }
}

fn toggle_explorer(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FileToggleExplorerBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(ExplorerToggleRequest::from(
        trigger.event().invocation().caller,
    ));
}

fn reveal_in_explorer(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FileRevealInExplorerBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(ExplorerRevealRequest::from(
        trigger.event().invocation().caller,
    ));
}

fn open_find(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FileFindBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(FileFindOpenRequest::new(
        trigger.event().invocation().caller,
        true,
    ));
}

fn open_find_in_files(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FileFindInFilesBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(ExplorerFindInFilesRequest::from(
        trigger.event().invocation().caller,
    ));
}

fn dispatch_panel_next_command(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FilePanelNextBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(FilePanelNextRequest {
        entity: trigger.event().invocation().caller,
    });
}

fn dispatch_panel_previous_command(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FilePanelPreviousBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(FilePanelPreviousRequest {
        entity: trigger.event().invocation().caller,
    });
}

fn dispatch_panel_choose_command(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FilePanelChooseBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(FilePanelChooseRequest {
        entity: trigger.event().invocation().caller,
        index: None,
    });
}

fn dispatch_panel_dismiss_command(
    trigger: On<CommandDispatch>,
    keys: Query<(), With<FilePanelDismissBinding>>,
    mut commands: Commands,
) {
    if !keys.contains(trigger.event().command()) {
        return;
    }
    commands.trigger(FilePanelDismissRequest {
        entity: trigger.event().invocation().caller,
    });
}

fn open_status_picker(
    trigger: On<UiInput<FileStatusPickerOpen>>,
    views: Query<(), With<FileView>>,
    locale: Option<Res<ResolvedLocale>>,
    mut open: MessageWriter<CommandBarOpenRequest>,
) {
    if !views.contains(trigger.event().webview) {
        return;
    }
    let picker = &trigger.event().payload.picker;
    if !EditorPicker::owns(&picker.0) {
        return;
    }
    let locale = locale
        .as_deref()
        .map(|locale| locale.0.clone())
        .unwrap_or_else(Locale::preferred);
    open.write(EditorPicks::request(picker.clone(), &locale));
}

fn pick_status(
    trigger: On<UiInput<PickRequest>>,
    focus: vmux_layout::stack::FocusedStack,
    locale: Option<Res<ResolvedLocale>>,
    mut picked: MessageWriter<FileStatusPicked>,
    mut open: MessageWriter<CommandBarOpenRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    match &trigger.event().payload.pick {
        CommandBarPick::Picker(next) => {
            let locale = locale
                .as_deref()
                .map(|locale| locale.0.clone())
                .unwrap_or_else(Locale::preferred);
            open.write(EditorPicks::request(next.clone(), &locale));
        }
        pick => {
            picked.write(FileStatusPicked {
                stack: focus.stack,
                pick: pick.clone(),
            });
        }
    }
    commands.trigger(CommandBarDismiss::new(webview, true));
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
            CommandBarPick::Typed { picker, value } if picker.is(EditorPicker::GOTO_LINE) => {
                let digits = value
                    .split_once(':')
                    .map_or(value.trim(), |(line, _)| line.trim());
                let Ok(line) = digits.parse::<u32>() else {
                    continue;
                };
                commands.trigger(UiInput {
                    webview: entity,
                    payload: ExplorerGoto {
                        path: String::new(),
                        line: line.saturating_sub(1),
                    },
                });
            }
            CommandBarPick::Typed { picker, value } if picker.is(EditorPicker::INDENT) => {
                let Some((kind, width)) = value.split_once(':') else {
                    continue;
                };
                let Ok(width) = width.parse::<u16>() else {
                    continue;
                };
                let spaces = match kind {
                    "spaces" => true,
                    "tabs" => false,
                    _ => continue,
                };
                let Ok(edit) = shapes.get(entity) else {
                    continue;
                };
                let shape = BufferShape::detect(&edit.core.buffer.rope);
                commands.trigger(UiInput {
                    webview: entity,
                    payload: FileShapeSet {
                        indent: FileIndent { spaces, width },
                        line_ending: shape.line_ending,
                    },
                });
            }
            CommandBarPick::Typed { picker, value } if picker.is(EditorPicker::LINE_ENDING) => {
                let Ok(edit) = shapes.get(entity) else {
                    continue;
                };
                let shape = BufferShape::detect(&edit.core.buffer.rope);
                let line_ending = match value.as_str() {
                    "crlf" => FileLineEnding::Crlf,
                    "lf" => FileLineEnding::Lf,
                    _ => continue,
                };
                commands.trigger(UiInput {
                    webview: entity,
                    payload: FileShapeSet {
                        indent: shape.indent,
                        line_ending,
                    },
                });
            }
            CommandBarPick::Typed { picker, value }
                if picker.is(EditorPicker::ENCODING_REOPEN)
                    || picker.is(EditorPicker::ENCODING_SAVE) =>
            {
                let Ok(encoding) = FileEncoding::try_from(value.as_str()) else {
                    continue;
                };
                if picker.is(EditorPicker::ENCODING_SAVE) {
                    commands.trigger(UiInput {
                        webview: entity,
                        payload: FileEncodingSaveRequest { encoding },
                    });
                } else {
                    commands.trigger(UiInput {
                        webview: entity,
                        payload: FileEncodingReopenRequest { encoding },
                    });
                }
            }
            CommandBarPick::Typed { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::CommandInvocation;
    #[derive(Resource, Default)]
    struct FindRequests(Vec<Entity>);

    #[derive(Resource, Default)]
    struct PanelRequests(Vec<Entity>);

    impl FindRequests {
        fn record(trigger: On<FileFindOpenRequest>, mut seen: ResMut<Self>) {
            seen.0.push(trigger.event_target());
        }
    }

    impl PanelRequests {
        fn record(trigger: On<FilePanelChooseRequest>, mut seen: ResMut<Self>) {
            seen.0.push(trigger.event_target());
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
                .init_resource::<FindRequests>()
                .init_resource::<PanelRequests>()
                .add_observer(FindRequests::record)
                .add_observer(PanelRequests::record);
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
    fn a_panel_key_reaches_host_ecs_for_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(&mut app, pressed, "file_panel_choose");

        assert_eq!(app.world().resource::<PanelRequests>().0, vec![pressed]);
        assert!(app.world().resource::<FindRequests>().0.is_empty());
        assert!(!app.world().resource::<PanelRequests>().0.contains(&other));
    }

    #[test]
    fn a_find_key_dispatches_a_targeted_ecs_request() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();

        Echo::issue(&mut app, pressed, "file_find");

        assert_eq!(app.world().resource::<FindRequests>().0, vec![pressed]);
    }

    #[derive(Resource, Default)]
    struct Reopened(Vec<(Entity, FileEncoding)>);

    impl Reopened {
        fn record(trigger: On<UiInput<FileEncodingReopenRequest>>, mut seen: ResMut<Self>) {
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
            CommandBarPick::Typed {
                picker: EditorPicker::encoding_reopen(),
                value: "Shift_JIS".to_string(),
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
                CommandBarPick::Typed {
                    picker: EditorPicker::encoding_reopen(),
                    value: "Shift_JIS".to_string(),
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
            CommandBarPick::Typed {
                picker: EditorPicker::encoding_reopen(),
                value: "Klingon".to_string(),
            },
        );

        assert!(app.world().resource::<Reopened>().0.is_empty());
    }
}
