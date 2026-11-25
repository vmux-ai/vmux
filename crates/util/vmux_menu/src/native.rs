use std::collections::HashMap;

use bevy_app::{App, Plugin, PostUpdate, Startup};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
#[cfg(target_os = "macos")]
use muda::ContextMenu as MudaContextMenu;
use muda::accelerator::Accelerator;
use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu as MudaSubmenu};
#[cfg(target_os = "macos")]
use objc2_foundation::MainThreadMarker;

use crate::{
    About, ContextMenu, Entry, EventSender, MenuBar, MenuPlugin, Predefined, Separator, Submenu,
    Transient,
};

pub struct NativeMenuPlugin;

impl Plugin for NativeMenuPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<MenuPlugin>() {
            app.add_plugins(MenuPlugin);
        }
        app.init_non_send::<NativeMenu>()
            .add_systems(Startup, install_handler)
            .add_systems(PostUpdate, (sync_bar, present_context).chain());
    }
}

#[derive(Default)]
struct NativeMenu {
    root_entity: Option<Entity>,
    menu_bar: Option<Menu>,
    submenus: HashMap<Entity, MudaSubmenu>,
    items: HashMap<Entity, MenuItem>,
    #[cfg(target_os = "macos")]
    context_menu: Option<Menu>,
    handler_installed: bool,
}

impl NativeMenu {
    fn available() -> bool {
        #[cfg(target_os = "macos")]
        {
            MainThreadMarker::new().is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    fn ensure_handler(&mut self, sender: &EventSender) {
        if self.handler_installed || !Self::available() {
            return;
        }
        let sender = sender.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            sender.send(event.id.0);
        }));
        self.handler_installed = true;
    }

    fn clear_bar(&mut self) {
        self.root_entity = None;
        self.menu_bar = None;
        self.submenus.clear();
        self.items.clear();
    }

    fn build_predefined(kind: Predefined, about: Option<&About>) -> PredefinedMenuItem {
        match kind {
            Predefined::About => PredefinedMenuItem::about(
                None,
                about.map(|metadata| AboutMetadata {
                    name: metadata.name.clone(),
                    version: metadata.version.clone(),
                    short_version: metadata.short_version.clone(),
                    copyright: metadata.copyright.clone(),
                    license: metadata.license.clone(),
                    website: metadata.website.clone(),
                    comments: metadata.comments.clone(),
                    credits: metadata.credits.clone(),
                    ..Default::default()
                }),
            ),
            Predefined::Undo => PredefinedMenuItem::undo(None),
            Predefined::Redo => PredefinedMenuItem::redo(None),
            Predefined::Cut => PredefinedMenuItem::cut(None),
            Predefined::Copy => PredefinedMenuItem::copy(None),
            Predefined::Paste => PredefinedMenuItem::paste(None),
            Predefined::SelectAll => PredefinedMenuItem::select_all(None),
            Predefined::Minimize => PredefinedMenuItem::minimize(None),
            Predefined::Maximize => PredefinedMenuItem::maximize(None),
            Predefined::Fullscreen => PredefinedMenuItem::fullscreen(None),
            Predefined::Hide => PredefinedMenuItem::hide(None),
            Predefined::HideOthers => PredefinedMenuItem::hide_others(None),
            Predefined::ShowAll => PredefinedMenuItem::show_all(None),
            Predefined::CloseWindow => PredefinedMenuItem::close_window(None),
            Predefined::Quit => PredefinedMenuItem::quit(None),
            Predefined::Services => PredefinedMenuItem::services(None),
            Predefined::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
        }
    }

    fn entry_id(entity: Entity, entry: &mut Entry, prefix: &str) -> String {
        if let Some(id) = entry.id() {
            return id.to_string();
        }
        let generated = format!("{prefix}_{}", entity.to_bits());
        entry.mark_presented(generated.clone());
        generated
    }

    fn build_item(entity: Entity, entry: &mut Entry, prefix: &str) -> MenuItem {
        let id = Self::entry_id(entity, entry, prefix);
        let accelerator = entry
            .accelerator()
            .and_then(|value| value.parse::<Accelerator>().ok());
        MenuItem::with_id(id, entry.label(), entry.enabled(), accelerator)
    }
}

#[derive(SystemParam)]
struct MenuTree<'w, 's> {
    submenus: Query<'w, 's, (Entity, Ref<'static, Submenu>, Option<&'static Children>)>,
    entries: Query<'w, 's, (Entity, &'static mut Entry)>,
    separators: Query<'w, 's, (), With<Separator>>,
    predefined: Query<'w, 's, (&'static Predefined, Option<&'static About>)>,
}

impl MenuTree<'_, '_> {
    fn submenu_snapshot(&self, entity: Entity) -> Option<(String, bool, Option<Vec<Entity>>)> {
        self.submenus.get(entity).ok().map(|(_, submenu, nested)| {
            (
                submenu.title().to_string(),
                submenu.enabled(),
                nested.map(|list| list.iter().collect::<Vec<_>>()),
            )
        })
    }

    fn populate_submenu(
        &mut self,
        native: &mut NativeMenu,
        parent: &MudaSubmenu,
        children: &[Entity],
    ) {
        for &child in children {
            if let Some((title, enabled, nested)) = self.submenu_snapshot(child) {
                let submenu = MudaSubmenu::new(title, enabled);
                if let Some(nested) = nested {
                    self.populate_submenu(native, &submenu, &nested);
                }
                let _ = parent.append(&submenu);
                native.submenus.insert(child, submenu);
                continue;
            }
            if let Ok((_, mut entry)) = self.entries.get_mut(child) {
                let item = NativeMenu::build_item(child, &mut entry, "vmux_menu_item");
                let _ = parent.append(&item);
                native.items.insert(child, item);
                continue;
            }
            if let Ok((kind, about)) = self.predefined.get(child) {
                let item = NativeMenu::build_predefined(*kind, about);
                let _ = parent.append(&item);
                continue;
            }
            if self.separators.contains(child) {
                let _ = parent.append(&PredefinedMenuItem::separator());
            }
        }
    }

    fn rebuild_bar(&mut self, native: &mut NativeMenu, bar_entity: Entity, children: &[Entity]) {
        native.clear_bar();
        let menu = Menu::new();
        for &child in children {
            if let Some((title, enabled, nested)) = self.submenu_snapshot(child) {
                let submenu = MudaSubmenu::new(title, enabled);
                if let Some(nested) = nested {
                    self.populate_submenu(native, &submenu, &nested);
                }
                let _ = menu.append(&submenu);
                native.submenus.insert(child, submenu);
                continue;
            }
            if let Ok((_, mut entry)) = self.entries.get_mut(child) {
                let item = NativeMenu::build_item(child, &mut entry, "vmux_menu_item");
                let _ = menu.append(&item);
                native.items.insert(child, item);
                continue;
            }
            if let Ok((kind, about)) = self.predefined.get(child) {
                let item = NativeMenu::build_predefined(*kind, about);
                let _ = menu.append(&item);
                continue;
            }
            if self.separators.contains(child) {
                let _ = menu.append(&PredefinedMenuItem::separator());
            }
        }
        #[cfg(target_os = "macos")]
        menu.init_for_nsapp();
        native.root_entity = Some(bar_entity);
        native.menu_bar = Some(menu);
    }

    fn mark_headless_tree(&mut self, children: &[Entity]) {
        for &child in children {
            let nested = self
                .submenus
                .get(child)
                .ok()
                .and_then(|(_, _, nested)| nested.map(|list| list.iter().collect::<Vec<_>>()));
            if let Some(nested) = nested {
                self.mark_headless_tree(&nested);
                continue;
            }
            if let Ok((_, mut entry)) = self.entries.get_mut(child) {
                let _ = NativeMenu::entry_id(child, &mut entry, "vmux_menu_item");
            }
        }
    }

    fn populate_context_submenu(
        &mut self,
        parent: &MudaSubmenu,
        children: &[Entity],
        commands: &mut Commands,
    ) {
        for &child in children {
            if let Some((title, enabled, nested)) = self.submenu_snapshot(child) {
                let submenu = MudaSubmenu::new(title, enabled);
                if let Some(nested) = nested {
                    self.populate_context_submenu(&submenu, &nested, commands);
                }
                let _ = parent.append(&submenu);
                continue;
            }
            if let Ok((_, mut entry)) = self.entries.get_mut(child) {
                let id = format!("os_context_menu_{}", child.to_bits());
                let item = MenuItem::with_id(id.clone(), entry.label(), entry.enabled(), None);
                let _ = parent.append(&item);
                entry.mark_presented(id);
                commands.entity(child).insert(Transient);
                continue;
            }
            if self.separators.contains(child) {
                let _ = parent.append(&PredefinedMenuItem::separator());
            }
        }
    }

    fn build_context_menu(&mut self, children: &[Entity], commands: &mut Commands) -> Menu {
        let menu = Menu::new();
        for &child in children {
            if let Some((title, enabled, nested)) = self.submenu_snapshot(child) {
                let submenu = MudaSubmenu::new(title, enabled);
                if let Some(nested) = nested {
                    self.populate_context_submenu(&submenu, &nested, commands);
                }
                let _ = menu.append(&submenu);
                continue;
            }
            if let Ok((_, mut entry)) = self.entries.get_mut(child) {
                let id = format!("os_context_menu_{}", child.to_bits());
                let item = MenuItem::with_id(id.clone(), entry.label(), entry.enabled(), None);
                let _ = menu.append(&item);
                entry.mark_presented(id);
                commands.entity(child).insert(Transient);
                continue;
            }
            if self.separators.contains(child) {
                let _ = menu.append(&PredefinedMenuItem::separator());
            }
        }
        menu
    }

    fn mark_headless_context(&mut self, children: &[Entity], commands: &mut Commands) {
        for &child in children {
            let nested = self
                .submenus
                .get(child)
                .ok()
                .and_then(|(_, _, nested)| nested.map(|list| list.iter().collect::<Vec<_>>()));
            if let Some(nested) = nested {
                self.mark_headless_context(&nested, commands);
                continue;
            }
            if let Ok((_, mut entry)) = self.entries.get_mut(child) {
                let id = format!("os_context_menu_{}", child.to_bits());
                entry.mark_presented(id);
                commands.entity(child).insert(Transient);
            }
        }
    }
}

type ChangedTreeChildren<'w, 's> =
    Query<'w, 's, (), (Or<(With<MenuBar>, With<Submenu>)>, Changed<Children>)>;

fn install_handler(mut native: NonSendMut<NativeMenu>, sender: Res<EventSender>) {
    native.ensure_handler(&sender);
}

fn sync_bar(
    mut native: NonSendMut<NativeMenu>,
    sender: Res<EventSender>,
    bars: Query<(Entity, Option<&Children>, Ref<MenuBar>)>,
    changed_children: ChangedTreeChildren,
    mut tree: MenuTree,
) {
    native.ensure_handler(&sender);
    let Some((bar_entity, children, bar)) = bars.iter().next() else {
        if native.root_entity.is_some() {
            native.clear_bar();
        }
        return;
    };
    let needs_rebuild =
        native.root_entity != Some(bar_entity) || bar.is_added() || !changed_children.is_empty();
    if needs_rebuild {
        let root_children = children
            .map(|list| list.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        if NativeMenu::available() {
            tree.rebuild_bar(&mut native, bar_entity, &root_children);
        } else {
            native.root_entity = Some(bar_entity);
            tree.mark_headless_tree(&root_children);
        }
        return;
    }
    if !NativeMenu::available() {
        return;
    }
    for (entity, submenu, _) in &tree.submenus {
        if !submenu.is_changed() {
            continue;
        }
        if let Some(handle) = native.submenus.get(&entity) {
            handle.set_text(submenu.title());
            handle.set_enabled(submenu.enabled());
        }
    }
    for (entity, entry) in &mut tree.entries {
        if !entry.is_changed() {
            continue;
        }
        if let Some(handle) = native.items.get(&entity) {
            handle.set_text(entry.label());
            handle.set_enabled(entry.enabled());
        }
    }
}

fn present_context(
    mut commands: Commands,
    mut native: NonSendMut<NativeMenu>,
    sender: Res<EventSender>,
    context_menus: Query<(&ContextMenu, &Children), Added<ContextMenu>>,
    mut tree: MenuTree,
) {
    native.ensure_handler(&sender);
    for (context, children) in &context_menus {
        let root_children = children.iter().collect::<Vec<_>>();
        if !NativeMenu::available() {
            tree.mark_headless_context(&root_children, &mut commands);
            continue;
        }
        let menu = tree.build_context_menu(&root_children, &mut commands);
        #[cfg(target_os = "macos")]
        {
            native.context_menu = Some(menu);
            if let Some(menu) = native.context_menu.as_ref() {
                unsafe {
                    menu.show_context_menu_for_nsview(context.view(), None);
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (context, menu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_menu_plugin_assigns_ids_and_marks_context_entries_transient() {
        let mut app = App::new();
        app.add_plugins(NativeMenuPlugin);

        let bar = app.world_mut().spawn(MenuBar).id();
        let sub = app
            .world_mut()
            .spawn((Submenu::new("File"), ChildOf(bar)))
            .id();
        let entry = app
            .world_mut()
            .spawn((Entry::new("Save", true), ChildOf(sub)))
            .id();
        let context = app
            .world_mut()
            .spawn(ContextMenu::new(std::ptr::null_mut()))
            .id();
        let context_entry = app
            .world_mut()
            .spawn((Entry::new("Inspect", true), ChildOf(context)))
            .id();

        app.update();

        let bar_id = app
            .world()
            .get::<Entry>(entry)
            .and_then(Entry::id)
            .map(str::to_string)
            .expect("menu bar entry receives a presented ID");
        let context_id = app
            .world()
            .get::<Entry>(context_entry)
            .and_then(Entry::id)
            .map(str::to_string)
            .expect("context entry receives a presented ID");
        assert!(app.world().get::<Transient>(context_entry).is_some());

        let sender = app.world().resource::<EventSender>().clone();
        sender.send(bar_id);
        sender.send(context_id);
        app.update();

        assert!(app.world().get_entity(entry).is_ok());
        assert!(app.world().get_entity(context_entry).is_err());
    }
}
