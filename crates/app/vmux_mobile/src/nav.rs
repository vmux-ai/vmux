use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

use crate::transition::{Level, NativeStack, TabEntry};

pub trait Route: Clone + PartialEq + Send + Sync + 'static {
    type Name: Copy + PartialEq + Send + Sync + 'static;

    fn name(&self) -> Self::Name;

    fn title(&self) -> String;

    fn is(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Component)]
pub struct Tab {
    pub id: String,
}

#[derive(Component)]
pub struct Local;

#[derive(Component)]
pub struct Selected;

#[derive(Component)]
pub struct Sheet;

#[derive(Component)]
pub struct Shows<S: Route>(pub S);

pub struct Seat<S: Route>(pub S);

impl<S: Route> Clone for Seat<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S: Route> Seat<S> {
    fn taken(screen: &S) -> vmux_native::Instance {
        let seated = Self(screen.clone());
        vmux_native::Instance::of(move |scope| scope.provide(seated))
    }
}

#[derive(Message)]
pub struct Report<S: Route> {
    pub tabs: Vec<(String, S)>,
    pub focused: Option<String>,
}

#[derive(Message)]
pub struct Select(pub String);

#[derive(Message)]
pub struct OpenBlank<S: Route>(pub S);

#[derive(Message)]
pub struct Push<S: Route>(pub S);

#[derive(Message)]
pub struct Present<S: Route>(pub S);

#[derive(Message)]
pub struct GoBack;

#[derive(Message)]
pub struct Dismiss;

#[derive(Message)]
pub struct Dropped(pub usize);

#[derive(Message)]
pub struct Tapped(pub &'static str);

#[derive(Clone, Copy, PartialEq)]
pub enum Arrives {
    Pushed,
    Presented,
}

#[derive(Message)]
pub struct Declare<S: Route> {
    pub name: S::Name,
    pub draws: &'static vmux_native::NativePage,
    pub arrives: Arrives,
    pub action: Option<&'static str>,
}

pub struct Draws {
    pub page: &'static vmux_native::NativePage,
    pub arrives: Arrives,
    pub action: Option<&'static str>,
}

#[derive(Resource)]
pub struct Declared<S: Route>(Vec<(S::Name, Draws)>);

impl<S: Route> Default for Declared<S> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<S: Route> Declared<S> {
    pub fn of(&self, name: S::Name) -> Option<&Draws> {
        for (known, draws) in &self.0 {
            if *known == name {
                return Some(draws);
            }
        }
        None
    }
}

#[derive(Resource)]
pub struct Centre(pub &'static str);

type Listed<'w, 's, S> = Query<
    'w,
    's,
    (
        &'static Tab,
        &'static Shows<S>,
        Option<&'static Local>,
        Option<&'static Selected>,
    ),
>;

#[derive(Resource, Default)]
struct Painted {
    tabs: Vec<TabEntry>,
    seated: Option<String>,
}

#[derive(Resource)]
struct Opened(u64);

pub struct NavPlugin<S: Route>(std::marker::PhantomData<S>);

impl<S: Route> Default for NavPlugin<S> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<S: Route> Plugin for NavPlugin<S> {
    fn build(&self, app: &mut App) {
        app.insert_resource(Opened(0))
            .init_resource::<Declared<S>>()
            .init_resource::<Painted>()
            .add_message::<Tapped>()
            .add_message::<Declare<S>>()
            .add_message::<Report<S>>()
            .add_message::<Select>()
            .add_message::<OpenBlank<S>>()
            .add_message::<Push<S>>()
            .add_message::<Present<S>>()
            .add_message::<GoBack>()
            .add_message::<Dismiss>()
            .add_message::<Dropped>()
            .add_systems(
                Update,
                (
                    Nav::reconcile,
                    Nav::declare::<S>,
                    Nav::report::<S>,
                    Nav::select,
                    Nav::open_blank::<S>,
                    Nav::stack::<S>,
                    Nav::unstack,
                    Nav::paint::<S>,
                )
                    .chain(),
            );
    }
}

#[derive(Clone, PartialEq)]
pub struct Entry<S: Route> {
    pub id: String,
    pub name: String,
    pub screen: S,
    pub local: bool,
}

#[derive(Clone, PartialEq)]
pub struct View<S: Route> {
    pub tabs: Vec<Entry<S>>,
    pub selected: Option<String>,
    pub root: Option<S>,
    pub current: Option<S>,
    pub depth: usize,
    pub sheet: bool,
}

impl<S: Route> Default for View<S> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            selected: None,
            root: None,
            current: None,
            depth: 0,
            sheet: false,
        }
    }
}

pub struct Nav;

impl Nav {
    pub fn view<S: Route>(world: &mut World) -> View<S> {
        let mut view = View::default();
        let mut tabs =
            world.query::<(Entity, &Tab, &Shows<S>, Option<&Local>, Option<&Selected>)>();
        let mut held = Vec::new();
        for (entity, tab, shows, local, selected) in tabs.iter(world) {
            held.push((entity, tab.id.clone(), shows.0.clone(), local.is_some()));
            if selected.is_some() {
                view.selected = Some(tab.id.clone());
            }
        }
        held.sort_by(|left, right| left.3.cmp(&right.3).then(left.1.cmp(&right.1)));

        let mut chosen = None;
        for (entity, id, screen, local) in held {
            if Some(&id) == view.selected.as_ref() {
                chosen = Some(entity);
                view.root = Some(screen.clone());
            }
            view.tabs.push(Entry {
                name: screen.title(),
                id,
                screen,
                local,
            });
        }

        let Some(tab) = chosen else {
            return view;
        };
        let mut children = world.query::<&Children>();
        let mut at = tab;
        while let Ok(kids) = children.get(world, at) {
            let Some(next) = kids.last().copied() else {
                break;
            };
            at = next;
            view.depth += 1;
        }
        view.sheet = world.get::<Sheet>(at).is_some();
        view.current = world.get::<Shows<S>>(at).map(|shows| shows.0.clone());
        view
    }

    pub fn top(tab: Entity, children: &Query<&Children>) -> Entity {
        let mut at = tab;
        while let Ok(kids) = children.get(at) {
            let Some(next) = kids.last().copied() else {
                break;
            };
            at = next;
        }
        at
    }

    fn reconcile(
        mut dropped: MessageWriter<Dropped>,
        mut tapped: MessageWriter<Tapped>,
        mut picked: MessageWriter<Select>,
        mut closed: MessageWriter<Dismiss>,
    ) {
        let count = crate::transition::take_popped() + crate::transition::take_dismissed();
        if count > 0 {
            dropped.write(Dropped(count));
        }
        for action in crate::transition::take_tapped() {
            tapped.write(Tapped(action));
        }
        if let Some(id) = crate::transition::take_picked() {
            picked.write(Select(id));
        }
        if crate::transition::take_closed() {
            closed.write(Dismiss);
        }
    }

    fn paint<S: Route>(
        known: Listed<S>,
        declared: Res<Declared<S>>,
        centre: Option<Res<Centre>>,
        mut painted: ResMut<Painted>,
        mut commands: Commands,
    ) {
        let mut listed = Vec::new();
        let mut selected = None;
        let mut ready = false;
        for (tab, shows, local, chosen) in known.iter() {
            listed.push((tab.id.clone(), shows.0.clone(), local.is_some()));
            if chosen.is_none() {
                continue;
            }
            selected = Some(tab.id.clone());
            ready = declared.of(shows.0.name()).is_some();
        }
        listed.sort_by(|left, right| left.2.cmp(&right.2).then(left.0.cmp(&right.0)));

        let mut at_selected = 0;
        for (at, (id, _, _)) in listed.iter().enumerate() {
            if Some(id) == selected.as_ref() {
                at_selected = at;
            }
        }

        let mut entries = Vec::new();
        let mut wanted = Vec::new();
        for (at, (id, screen, _)) in listed.into_iter().enumerate() {
            let here = Some(&id) == selected.as_ref();
            if at.abs_diff(at_selected) <= 1
                && let Some(draws) = declared.of(screen.name())
            {
                wanted.push((
                    id.clone(),
                    Level {
                        page: draws.page,
                        title: screen.title(),
                        action: draws.action,
                        closable: false,
                        seat: Seat::taken(&screen),
                    },
                ));
            }
            entries.push(TabEntry {
                id,
                name: screen.title(),
                here,
            });
        }
        if ready {
            NativeStack::warm(wanted);
        }

        if ready && painted.seated != selected {
            painted.seated = selected.clone();
            if let Some(id) = selected.clone() {
                commands.queue(move |world: &mut World| {
                    NativeStack::settle(id.clone(), Nav::levels::<S>(world, &id));
                });
            }
        }
        if painted.tabs == entries {
            return;
        }
        painted.tabs = entries.clone();
        NativeStack::tabs(entries, centre.map(|centre| centre.0));
    }

    fn declare<S: Route>(mut asked: MessageReader<Declare<S>>, mut declared: ResMut<Declared<S>>) {
        for Declare {
            name,
            draws,
            arrives,
            action,
        } in asked.read()
        {
            if declared.of(*name).is_some() {
                continue;
            }
            declared.0.push((
                *name,
                Draws {
                    page: draws,
                    arrives: *arrives,
                    action: *action,
                },
            ));
        }
    }

    fn report<S: Route>(
        mut reported: MessageReader<Report<S>>,
        known: Query<(Entity, &Tab, Option<&Local>, &Shows<S>)>,
        selected: Query<Entity, With<Selected>>,
        mut commands: Commands,
    ) {
        let Some(report) = reported.read().last() else {
            return;
        };

        for (entity, tab, local, shows) in known.iter() {
            let still_open = report.tabs.iter().any(|(id, _)| *id == tab.id);
            let superseded =
                local.is_some() && report.tabs.iter().any(|(_, screen)| shows.0.is(screen));
            if local.is_some() {
                if superseded {
                    commands.entity(entity).despawn();
                }
                continue;
            }
            if !still_open {
                commands.entity(entity).despawn();
            }
        }

        for (id, screen) in &report.tabs {
            if known.iter().any(|(_, tab, _, _)| tab.id == *id) {
                continue;
            }
            commands.spawn((Tab { id: id.clone() }, Shows(screen.clone())));
        }

        let holds = selected.iter().next().is_some();
        let lands_on = report
            .focused
            .clone()
            .filter(|id| report.tabs.iter().any(|(known, _)| known == id))
            .or_else(|| report.tabs.first().map(|(id, _)| id.clone()));
        if !holds && let Some(id) = lands_on {
            commands.queue(move |world: &mut World| Nav::mark(world, &id));
        }
    }

    fn mark(world: &mut World, id: &str) {
        let mut found = None;
        let mut query = world.query::<(Entity, &Tab)>();
        for (entity, tab) in query.iter(world) {
            if tab.id == id {
                found = Some(entity);
                break;
            }
        }
        let Some(entity) = found else {
            return;
        };
        let mut holders = world.query_filtered::<Entity, With<Selected>>();
        let previous: Vec<Entity> = holders.iter(world).collect();
        for holder in previous {
            world.entity_mut(holder).remove::<Selected>();
        }
        world.entity_mut(entity).insert(Selected);
    }

    fn select(mut asked: MessageReader<Select>, mut commands: Commands) {
        let Some(Select(id)) = asked.read().last() else {
            return;
        };
        let id = id.clone();
        commands.queue(move |world: &mut World| Nav::mark(world, &id));
    }

    fn levels<S: Route>(world: &mut World, id: &str) -> Vec<Level> {
        let mut tabs = world.query::<(Entity, &Tab)>();
        let Some(mut at) = tabs
            .iter(world)
            .find(|(_, tab)| tab.id == id)
            .map(|(entity, _)| entity)
        else {
            return Vec::new();
        };
        let at_tab = at;
        let mut chain = Vec::new();
        let mut children = world.query::<&Children>();
        while let Ok(kids) = children.get(world, at) {
            let Some(next) = kids.last().copied() else {
                break;
            };
            chain.push(next);
            at = next;
        }

        let mut shown = Vec::new();
        if let Some(shows) = world.get::<Shows<S>>(at_tab) {
            shown.push(shows.0.clone());
        }
        for entity in chain {
            let Some(shows) = world.get::<Shows<S>>(entity) else {
                continue;
            };
            shown.push(shows.0.clone());
        }
        let Some(declared) = world.get_resource::<Declared<S>>() else {
            return Vec::new();
        };
        let mut levels = Vec::new();
        for screen in shown {
            let Some(draws) = declared.of(screen.name()) else {
                continue;
            };
            levels.push(Level {
                page: draws.page,
                title: screen.title(),
                action: draws.action,
                closable: false,
                seat: Seat::taken(&screen),
            });
        }
        levels
    }

    fn open_blank<S: Route>(
        mut asked: MessageReader<OpenBlank<S>>,
        mut opened: ResMut<Opened>,
        mut commands: Commands,
    ) {
        for OpenBlank(screen) in asked.read() {
            let ordinal = opened.0;
            opened.0 = ordinal.wrapping_add(1);
            let id = format!("local:{ordinal}");
            commands.spawn((Tab { id: id.clone() }, Local, Shows(screen.clone())));
            commands.queue(move |world: &mut World| Nav::mark(world, &id));
        }
    }

    fn stack<S: Route>(
        mut pushes: MessageReader<Push<S>>,
        mut presents: MessageReader<Present<S>>,
        declared: Res<Declared<S>>,
        selected: Query<Entity, With<Selected>>,
        children: Query<&Children>,
        mut commands: Commands,
    ) {
        let Some(tab) = selected.iter().next() else {
            return;
        };
        for Push(screen) in pushes.read() {
            let onto = Self::top(tab, &children);
            commands.spawn((Shows(screen.clone()), ChildOf(onto)));
            let Some(draws) = declared.of(screen.name()) else {
                continue;
            };
            NativeStack::push(Level {
                page: draws.page,
                title: screen.title(),
                action: draws.action,
                closable: false,
                seat: Seat::taken(screen),
            });
        }
        for Present(screen) in presents.read() {
            let onto = Self::top(tab, &children);
            commands.spawn((Shows(screen.clone()), Sheet, ChildOf(onto)));
            let Some(draws) = declared.of(screen.name()) else {
                continue;
            };
            NativeStack::present(Level {
                page: draws.page,
                title: screen.title(),
                action: draws.action,
                closable: true,
                seat: Seat::taken(screen),
            });
        }
    }

    fn unstack(
        mut backs: MessageReader<GoBack>,
        mut dismisses: MessageReader<Dismiss>,
        mut dropped: MessageReader<Dropped>,
        selected: Query<Entity, With<Selected>>,
        children: Query<&Children>,
        sheets: Query<&Sheet>,
        mut commands: Commands,
    ) {
        let Some(tab) = selected.iter().next() else {
            return;
        };
        let backs = backs.read().count();
        let dismisses = dismisses.read().count();
        let mut already_gone = 0;
        for Dropped(count) in dropped.read() {
            already_gone += count;
        }
        let asked = backs + dismisses + already_gone;
        if asked == 0 {
            return;
        }

        let mut chain = Vec::new();
        let mut at = tab;
        while let Ok(kids) = children.get(at) {
            let Some(next) = kids.last().copied() else {
                break;
            };
            chain.push(next);
            at = next;
        }

        for level in chain.iter().rev().take(backs + dismisses) {
            if sheets.get(*level).is_ok() {
                NativeStack::dismiss();
            } else {
                NativeStack::pop();
            }
        }

        let keep = chain.len().saturating_sub(asked);
        if let Some(deepest_kept) = chain.get(keep) {
            commands.entity(*deepest_kept).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq)]
    enum PageName {
        Home,
        Note,
        Unsaved,
    }

    #[derive(Clone, Debug, PartialEq)]
    enum Page {
        Home,
        Note(&'static str),
        Unsaved,
    }

    impl Route for Page {
        type Name = PageName;

        fn name(&self) -> PageName {
            match self {
                Self::Home => PageName::Home,
                Self::Note(_) => PageName::Note,
                Self::Unsaved => PageName::Unsaved,
            }
        }

        fn title(&self) -> String {
            match self {
                Self::Home => "Home".to_string(),
                Self::Note(name) => (*name).to_string(),
                Self::Unsaved => "Unsaved".to_string(),
            }
        }

        fn is(&self, other: &Self) -> bool {
            matches!((self, other), (Self::Unsaved, Self::Note(_)))
        }
    }

    struct Phone(App);

    impl Phone {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(NavPlugin::<Page>::default());
            Self(app)
        }

        fn reports(&mut self, tabs: &[(&str, Page)], focused: Option<&str>) -> &mut Self {
            let mut carried = Vec::new();
            for (id, screen) in tabs {
                carried.push(((*id).to_string(), screen.clone()));
            }
            self.0.world_mut().write_message(Report {
                tabs: carried,
                focused: focused.map(str::to_string),
            });
            self.turn()
        }

        fn sends<M: Message>(&mut self, message: M) -> &mut Self {
            self.0.world_mut().write_message(message);
            self.turn()
        }

        fn turn(&mut self) -> &mut Self {
            self.0.update();
            self
        }

        fn tabs(&mut self) -> Vec<String> {
            let mut query = self.0.world_mut().query::<&Tab>();
            let mut ids: Vec<String> = query
                .iter(self.0.world())
                .map(|tab| tab.id.clone())
                .collect();
            ids.sort();
            ids
        }

        fn selected(&mut self) -> Option<String> {
            let mut query = self.0.world_mut().query_filtered::<&Tab, With<Selected>>();
            query.iter(self.0.world()).next().map(|tab| tab.id.clone())
        }

        fn depth(&mut self) -> usize {
            let Some(id) = self.selected() else {
                return 0;
            };
            let mut tabs = self.0.world_mut().query::<(Entity, &Tab)>();
            let entity = tabs
                .iter(self.0.world())
                .find(|(_, tab)| tab.id == id)
                .map(|(entity, _)| entity);
            let Some(mut entity) = entity else {
                return 0;
            };
            let mut depth = 0;
            let mut children = self.0.world_mut().query::<&Children>();
            while let Ok(kids) = children.get(self.0.world(), entity) {
                let Some(next) = kids.last().copied() else {
                    break;
                };
                entity = next;
                depth += 1;
            }
            depth
        }
    }

    #[test]
    fn a_reported_tab_becomes_an_entity_and_the_focused_one_is_selected() {
        let mut phone = Phone::new();
        phone.reports(
            &[("tab:1", Page::Home), ("tab:2", Page::Note("Second"))],
            Some("tab:2"),
        );
        assert_eq!(phone.tabs(), vec!["tab:1", "tab:2"]);
        assert_eq!(phone.selected().as_deref(), Some("tab:2"));
    }

    #[test]
    fn landing_falls_back_to_the_first_when_the_focus_is_unknown() {
        let mut phone = Phone::new();
        phone.reports(&[("tab:1", Page::Home)], Some("tab:99"));
        assert_eq!(phone.selected().as_deref(), Some("tab:1"));
    }

    #[test]
    fn a_tab_the_mac_stops_reporting_is_despawned() {
        let mut phone = Phone::new();
        phone.reports(&[("tab:1", Page::Home), ("tab:2", Page::Home)], None);
        phone.reports(&[("tab:1", Page::Home)], None);
        assert_eq!(phone.tabs(), vec!["tab:1"]);
    }

    #[test]
    fn a_pushed_level_dies_with_the_tab_that_held_it() {
        let mut phone = Phone::new();
        phone.reports(&[("tab:1", Page::Home)], None);
        phone.sends(Push(Page::Note("Deeper")));
        assert_eq!(phone.depth(), 1);

        phone.reports(&[("tab:2", Page::Home)], None);
        let mut shown = phone.0.world_mut().query::<&Shows<Page>>();
        let left: Vec<Page> = shown
            .iter(phone.0.world())
            .map(|shows| shows.0.clone())
            .collect();
        assert_eq!(left, vec![Page::Home], "the pushed level went with its tab");
    }

    #[test]
    fn back_pops_one_level_and_stops_at_the_root() {
        let mut phone = Phone::new();
        phone.reports(&[("tab:1", Page::Home)], None);
        phone.sends(Push(Page::Note("One")));
        phone.sends(Push(Page::Note("Two")));
        assert_eq!(phone.depth(), 2);

        phone.sends(GoBack);
        assert_eq!(phone.depth(), 1);
        phone.sends(GoBack);
        phone.sends(GoBack);
        assert_eq!(phone.depth(), 0);
        assert_eq!(phone.tabs(), vec!["tab:1"], "the tab itself survives");
    }

    #[test]
    fn a_swipe_reports_what_uikit_already_dropped() {
        let mut phone = Phone::new();
        phone.reports(&[("tab:1", Page::Home)], None);
        phone.sends(Push(Page::Note("One")));
        phone.sends(Push(Page::Note("Two")));
        phone.sends(Dropped(2));
        assert_eq!(phone.depth(), 0);
    }

    #[test]
    fn modals_stack_on_top_of_the_pushed_level() {
        let mut phone = Phone::new();
        phone.reports(&[("tab:1", Page::Home)], None);
        phone.sends(Push(Page::Note("Behind")));
        phone.sends(Present(Page::Note("First sheet")));
        phone.sends(Present(Page::Note("Second sheet")));
        assert_eq!(phone.depth(), 3);

        phone.sends(Dismiss);
        assert_eq!(phone.depth(), 2);
    }

    #[test]
    fn depth_is_kept_per_tab() {
        let mut phone = Phone::new();
        phone.reports(
            &[("tab:1", Page::Home), ("tab:2", Page::Home)],
            Some("tab:1"),
        );
        phone.sends(Push(Page::Note("Only in one")));
        assert_eq!(phone.depth(), 1);

        phone.sends(Select("tab:2".to_string()));
        assert_eq!(phone.depth(), 0);

        phone.sends(Select("tab:1".to_string()));
        assert_eq!(phone.depth(), 1);
    }

    #[test]
    fn a_local_tab_gives_way_once_the_same_screen_is_reported() {
        let mut phone = Phone::new();
        phone.sends(OpenBlank(Page::Unsaved));
        assert_eq!(phone.tabs(), vec!["local:0"]);

        phone.reports(&[("tab:1", Page::Note("Saved"))], None);
        assert_eq!(phone.tabs(), vec!["tab:1"]);
    }

    #[test]
    fn a_local_tab_the_mac_never_reports_survives_every_poll() {
        let mut phone = Phone::new();
        phone.sends(OpenBlank(Page::Home));
        phone.reports(&[("tab:1", Page::Note("Unrelated"))], None);
        assert_eq!(phone.tabs(), vec!["local:0", "tab:1"]);
    }
}
