use std::marker::PhantomData;

use bevy::prelude::*;

use crate::shortcut::{Binding, KeyCombo, Modifiers, Shortcut, Source, When, resolve_key};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutDefinition {
    Direct(String),
    Chord(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandShortcut {
    pub shortcut: ShortcutDefinition,
    pub when: Option<String>,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct CommandDefinition {
    pub id: String,
    pub label: String,
    pub group: String,
    pub accelerator: Option<String>,
    pub hidden: bool,
    pub native_menu: bool,
    pub shortcut_label: Option<String>,
    pub shortcuts: Vec<CommandShortcut>,
}

impl CommandDefinition {
    pub fn command_bar_name(&self) -> String {
        format!("{} > {}", self.group, self.label)
    }

    pub fn shortcut_label(&self) -> String {
        if let Some(label) = &self.shortcut_label {
            return label.clone();
        }
        self.bindings()
            .into_iter()
            .next()
            .map(|binding| binding.shortcut.display())
            .unwrap_or_default()
    }

    pub fn bindings(&self) -> Vec<Binding> {
        let mut bindings = Vec::new();
        for definition in &self.shortcuts {
            let shortcut = match &definition.shortcut {
                ShortcutDefinition::Direct(value) => {
                    let Some(combo) = KeyCombo::parse(value) else {
                        continue;
                    };
                    Shortcut::Direct(combo)
                }
                ShortcutDefinition::Chord(value) => {
                    let Some((prefix, second)) = value.split_once(',') else {
                        continue;
                    };
                    let (Some(prefix), Some(second)) = (
                        KeyCombo::parse(prefix.trim()),
                        KeyCombo::parse(second.trim()),
                    ) else {
                        continue;
                    };
                    Shortcut::Chord(prefix, second)
                }
            };
            bindings.push(Binding {
                shortcut,
                command: self.id.to_string(),
                when: definition.when.as_deref().and_then(When::parse),
            });
        }
        if bindings.is_empty()
            && let Some(accelerator) = &self.accelerator
            && let Some(combo) = KeyCombo::parse(accelerator)
        {
            bindings.push(Binding {
                shortcut: Shortcut::Direct(combo),
                command: self.id.to_string(),
                when: None,
            });
        }
        bindings
    }

    pub fn default_shortcuts(definitions: &[Self]) -> Vec<Binding> {
        let mut bindings = Vec::new();
        for definition in definitions {
            bindings.extend(definition.bindings());
        }
        bindings
    }

    pub fn extend_keymap(definitions: &[Self], keymap: &mut crate::shortcut::Keymap) {
        keymap.register(definitions.iter().map(|definition| definition.id.as_str()));
        keymap.extend(Source::Default, Self::default_shortcuts(definitions));
    }

    pub fn append_native_menus(
        definitions: &[Self],
        menu: &mut muda::Menu,
    ) -> Result<(), muda::Error> {
        for definition in definitions {
            if definition.hidden || !definition.native_menu {
                continue;
            }
            let mut path = definition.group.split(" > ");
            let Some(root_name) = path.next() else {
                continue;
            };
            let existing = menu
                .items()
                .into_iter()
                .filter_map(|item| item.as_submenu().cloned())
                .find(|submenu| submenu.text() == root_name);
            let mut submenu = if let Some(existing) = existing {
                existing
            } else {
                let created = muda::Submenu::new(root_name, true);
                menu.append(&created)?;
                created
            };
            for name in path {
                let existing = submenu
                    .items()
                    .into_iter()
                    .filter_map(|item| item.as_submenu().cloned())
                    .find(|child| child.text() == name);
                submenu = if let Some(existing) = existing {
                    existing
                } else {
                    let created = muda::Submenu::new(name, true);
                    submenu.append(&created)?;
                    created
                };
            }
            let accelerator = definition
                .accelerator
                .as_deref()
                .and_then(|value| value.parse::<muda::accelerator::Accelerator>().ok());
            let item =
                muda::MenuItem::with_id(&definition.id, &definition.label, true, accelerator);
            submenu.append(&item)?;
        }
        Ok(())
    }
}

#[derive(Message, Clone, Debug, PartialEq)]
pub struct CommandInvocation {
    pub caller: Entity,
    pub id: String,
    pub arguments: serde_json::Value,
}

impl CommandInvocation {
    pub fn new(caller: Entity, id: impl Into<String>) -> Self {
        Self {
            caller,
            id: id.into(),
            arguments: serde_json::Value::Null,
        }
    }
}

pub trait RegisteredCommand: Message + Sized {
    fn definition() -> CommandDefinition;
    fn from_invocation(invocation: &CommandInvocation) -> Option<Self>;
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchCommandInvocations;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegisterCommandDefinitions;

pub struct CommandRequestPlugin<T>(PhantomData<fn() -> T>);

impl<T> Default for CommandRequestPlugin<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: RegisteredCommand> Plugin for CommandRequestPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_message::<CommandInvocation>()
            .add_message::<T>()
            .add_systems(
                Startup,
                spawn_definition::<T>.in_set(RegisterCommandDefinitions),
            )
            .add_systems(
                Update,
                dispatch_command::<T>.in_set(DispatchCommandInvocations),
            );
    }
}

fn spawn_definition<T: RegisteredCommand>(mut commands: Commands) {
    commands.spawn(T::definition());
}

fn dispatch_command<T: RegisteredCommand>(
    mut invocations: MessageReader<CommandInvocation>,
    mut requests: MessageWriter<T>,
) {
    for invocation in invocations.read() {
        if let Some(request) = T::from_invocation(invocation) {
            requests.write(request);
        }
    }
}

impl KeyCombo {
    fn parse(value: &str) -> Option<Self> {
        let mut modifiers = Modifiers::default();
        let mut key = None;
        for part in value.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "shift" => modifiers.shift = true,
                "alt" | "option" => modifiers.alt = true,
                "super" | "cmd" | "command" | "meta" | "cmdorctrl" => {
                    modifiers.super_key = true;
                }
                _ => {
                    if key.is_some() {
                        return None;
                    }
                    let resolved = resolve_key(part)?;
                    key = Some(resolved.key);
                    modifiers.shift |= resolved.implicit_shift;
                }
            }
        }
        Some(Self {
            key: key?,
            modifiers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Message, vmux_macro::CommandBarRequest, Clone, Copy, Debug, PartialEq, Eq)]
    #[command_bar(
        id = "test_toggle",
        label = "Toggle",
        group = "Test",
        accel = "super+t"
    )]
    struct TestToggle;

    #[test]
    fn registered_command_dispatches_to_its_typed_message() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<CommandInvocation>()
            .add_plugins(CommandRequestPlugin::<TestToggle>::default());
        let caller = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(caller, "test_toggle"));

        app.update();

        let requests = app
            .world_mut()
            .resource_mut::<Messages<TestToggle>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests, [TestToggle]);
    }

    #[test]
    fn registered_command_contributes_metadata_and_shortcuts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CommandRequestPlugin::<TestToggle>::default());
        app.update();
        let mut query = app.world_mut().query::<&CommandDefinition>();
        let definitions = query.iter(app.world()).cloned().collect::<Vec<_>>();
        let definition = definitions
            .iter()
            .find(|definition| definition.id == "test_toggle")
            .unwrap();

        assert_eq!(definition.command_bar_name(), "Test > Toggle");
        assert_eq!(definition.shortcut_label(), "⌘T");
        assert_eq!(
            CommandDefinition::default_shortcuts(&definitions)[0].command,
            "test_toggle"
        );
    }
}
