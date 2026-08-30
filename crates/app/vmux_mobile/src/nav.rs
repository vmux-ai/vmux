use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

pub use crate::transition::Presentation;
use crate::transition::{Level, NativeStack, ROTATE, ROTATE_BACK, TabItem};
pub use vmux_macro::Route;

pub trait ScreenName: Copy + PartialEq + Send + Sync + 'static {
    type Route: Route<Name = Self>;
}

pub trait Route: Clone + PartialEq + std::hash::Hash + Send + Sync + 'static {
    type Name: ScreenName<Route = Self>;

    fn name(&self) -> Self::Name;

    fn title(&self) -> String;

    fn key(&self) -> u64 {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    fn blank(_at: usize) -> Option<Self> {
        None
    }

    fn is(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Component)]
pub struct Tab {
    pub id: String,
}

#[derive(Component)]
pub struct Local(pub u64);

#[derive(Component)]
pub struct Selected;

#[derive(Component)]
pub struct Presented;

#[derive(Component)]
pub struct Depth(pub usize);

#[derive(Component)]
pub struct Warming(u8);

#[derive(Resource, Default)]
pub struct Warm(Vec<Entity>);

#[derive(Resource, Default)]
pub struct Trail(pub Vec<Entity>);

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
pub struct Sprout;

#[derive(Message)]
pub struct Close(pub String);

#[derive(Message)]
pub struct Push<S: Route>(pub S);

#[derive(Message)]
pub struct Present<S: Route>(pub S);

#[derive(Message)]
pub struct Prefetch<S: Route>(pub S);

#[derive(Message)]
pub struct GoBack;

#[derive(Message)]
pub struct Dismiss;

#[derive(Message)]
pub struct Dropped(pub usize);

#[derive(Message)]
pub struct Tapped(pub &'static str);

#[derive(Message)]
pub struct Declare<S: Route> {
    pub name: S::Name,
    pub component: &'static vmux_native::NativePage,
    pub presentation: Presentation,
    pub detents: &'static [f64],
}

#[derive(PartialEq)]
pub struct ScreenPage<N: ScreenName> {
    pub page: &'static vmux_native::NativePage,
    pub name: N,
    pub presentation: Presentation,
    pub detents: &'static [f64],
}

pub struct ScreenOptions {
    pub component: &'static vmux_native::NativePage,
    pub presentation: Presentation,
    pub detents: &'static [f64],
}

#[derive(Resource)]
pub struct Screens<S: Route>(Vec<(S::Name, ScreenOptions)>);

impl<S: Route> Default for Screens<S> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<S: Route> Screens<S> {
    pub fn of(&self, name: S::Name) -> Option<&ScreenOptions> {
        for (known, options) in &self.0 {
            if *known == name {
                return Some(options);
            }
        }
        None
    }
}

#[derive(Resource)]
pub struct Centre(pub &'static str);

const NEW_TAB: &str = "+";
const WARM_SETTLE: u8 = 2;
const WARM_MOST: usize = 6;

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
    tabs: Vec<TabItem>,
    seated: Option<String>,
    turn: u64,
}

#[derive(Resource, Default)]
struct Turns(u64);

#[derive(Resource)]
struct Opened(u64);

enum Landing {
    Seated,
    Beside,
}

pub struct NavPlugin<S: Route>(std::marker::PhantomData<S>);

impl<S: Route> Default for NavPlugin<S> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<S: Route> Plugin for NavPlugin<S> {
    fn build(&self, app: &mut App) {
        app.insert_resource(Opened(0))
            .init_resource::<Screens<S>>()
            .init_resource::<Painted>()
            .init_resource::<Turns>()
            .init_resource::<Trail>()
            .init_resource::<Warm>()
            .add_message::<Tapped>()
            .add_message::<Declare<S>>()
            .add_message::<Report<S>>()
            .add_message::<Select>()
            .add_message::<OpenBlank<S>>()
            .add_message::<Sprout>()
            .add_message::<Close>()
            .add_message::<Push<S>>()
            .add_message::<Present<S>>()
            .add_message::<Prefetch<S>>()
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
                    Nav::close,
                    Nav::open_blank::<S>,
                    Nav::stack::<S>,
                    Nav::unstack,
                    Nav::rotate,
                    Nav::measure,
                    Nav::warm::<S>,
                    Nav::paint::<S>,
                )
                    .chain(),
            );
    }
}

#[derive(Clone, PartialEq)]
pub struct TabRoute<S: Route> {
    pub id: String,
    pub name: String,
    pub screen: S,
    pub local: bool,
}

#[derive(Clone, PartialEq)]
pub struct NavigationState<S: Route> {
    pub tabs: Vec<TabRoute<S>>,
    pub selected: Option<String>,
    pub root: Option<S>,
    pub current: Option<S>,
    pub trail: Vec<S>,
    pub depth: usize,
    pub sheet: bool,
}

impl<S: Route> Default for NavigationState<S> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            selected: None,
            root: None,
            current: None,
            trail: Vec::new(),
            depth: 0,
            sheet: false,
        }
    }
}

pub struct Nav;

impl Nav {
    pub fn state<S: Route>(world: &mut World) -> NavigationState<S> {
        let mut state = NavigationState::default();
        let mut tabs =
            world.query::<(Entity, &Tab, &Shows<S>, Option<&Local>, Option<&Selected>)>();
        let mut held = Vec::new();
        for (entity, tab, shows, local, selected) in tabs.iter(world) {
            held.push((
                entity,
                tab.id.clone(),
                shows.0.clone(),
                local.map(|it| it.0),
            ));
            if selected.is_some() {
                state.selected = Some(tab.id.clone());
            }
        }
        held.sort_by(|left, right| left.3.cmp(&right.3).then(left.1.cmp(&right.1)));

        let mut chosen = None;
        for (entity, id, screen, local) in held {
            if Some(&id) == state.selected.as_ref() {
                chosen = Some(entity);
                state.root = Some(screen.clone());
                state.trail.push(screen.clone());
            }
            state.tabs.push(TabRoute {
                name: screen.title(),
                id,
                screen,
                local: local.is_some(),
            });
        }

        let Some(tab) = chosen else {
            return state;
        };
        let mut children = world.query::<&Children>();
        let mut at = tab;
        while let Ok(kids) = children.get(world, at) {
            let Some(next) = kids.last().copied() else {
                break;
            };
            at = next;
            state.depth += 1;
            if let Some(shows) = world.get::<Shows<S>>(next) {
                state.trail.push(shows.0.clone());
            }
        }
        state.sheet = world.get::<Presented>(at).is_some();
        state.current = world.get::<Shows<S>>(at).map(|shows| shows.0.clone());
        state
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
        mut closing: MessageWriter<Close>,
        mut sprouting: MessageWriter<Sprout>,
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
        for id in crate::transition::take_closing() {
            closing.write(Close(id));
        }
        if crate::transition::take_sprouting() {
            sprouting.write(Sprout);
        }
    }

    fn measure(
        selected: Query<Entity, (With<Tab>, With<Selected>)>,
        children: Query<&Children>,
        measured: Query<Entity, (With<Depth>, Without<Warming>)>,
        mut trail: ResMut<Trail>,
        mut commands: Commands,
    ) {
        let mut chain = Vec::new();
        if let Some(tab) = selected.iter().next() {
            chain.push(tab);
            let mut at = tab;
            while let Ok(kids) = children.get(at) {
                let Some(next) = kids.last().copied() else {
                    break;
                };
                chain.push(next);
                at = next;
            }
        }
        for entity in measured.iter() {
            if !chain.contains(&entity) {
                commands.entity(entity).remove::<Depth>();
            }
        }
        for (at, entity) in chain.iter().copied().enumerate() {
            commands.entity(entity).insert(Depth(at));
        }
        if trail.0 != chain {
            trail.0 = chain;
        }
    }

    fn warmed<S: Route>(
        screen: &S,
        warming: &Query<(Entity, &Shows<S>), With<Warming>>,
    ) -> Option<Entity> {
        for (entity, shows) in warming.iter() {
            if shows.0.is(screen) {
                return Some(entity);
            }
        }
        None
    }

    fn warm<S: Route>(
        mut asked: MessageReader<Prefetch<S>>,
        mut warming: Query<(&Shows<S>, &mut Warming)>,
        known: Query<&Shows<S>>,
        screens: Res<Screens<S>>,
        trail: Res<Trail>,
        mut warm: ResMut<Warm>,
        mut commands: Commands,
    ) {
        warm.0.retain(|entity| warming.contains(*entity));
        while warm.0.len() > WARM_MOST {
            let oldest = warm.0.remove(0);
            commands.entity(oldest).despawn();
        }

        for (shows, mut settling) in warming.iter_mut() {
            if settling.0 >= WARM_SETTLE {
                continue;
            }
            settling.0 += 1;
            if settling.0 < WARM_SETTLE {
                continue;
            }
            let Some(options) = screens.of(shows.0.name()) else {
                continue;
            };
            NativeStack::stow(Level {
                key: shows.0.key(),
                page: options.component,
                title: shows.0.title(),
                presentation: options.presentation,
                detents: options.detents,
                seat: Seat::taken(&shows.0),
            });
        }

        for Prefetch(screen) in asked.read() {
            if screens.of(screen.name()).is_none() {
                continue;
            }
            let mut standing = false;
            for shows in known.iter() {
                if shows.0.is(screen) {
                    standing = true;
                    break;
                }
            }
            if standing {
                continue;
            }
            let fresh = commands
                .spawn((Shows(screen.clone()), Warming(0), Depth(trail.0.len())))
                .id();
            warm.0.push(fresh);
        }
    }

    fn paint<S: Route>(
        known: Listed<S>,
        screens: Res<Screens<S>>,
        centre: Option<Res<Centre>>,
        turns: Res<Turns>,
        mut painted: ResMut<Painted>,
        mut commands: Commands,
    ) {
        let mut listed = Vec::new();
        let mut selected = None;
        let mut ready = false;
        for (tab, shows, local, chosen) in known.iter() {
            listed.push((tab.id.clone(), shows.0.clone(), local.map(|it| it.0)));
            if chosen.is_none() {
                continue;
            }
            selected = Some(tab.id.clone());
            ready = screens.of(shows.0.name()).is_some();
        }
        listed.sort_by(|left, right| left.2.cmp(&right.2).then(left.0.cmp(&right.0)));

        let mut at_selected = 0;
        for (at, (id, _, _)) in listed.iter().enumerate() {
            if Some(id) == selected.as_ref() {
                at_selected = at;
            }
        }

        let mut entries = Vec::new();
        let mut beside = Vec::new();
        for (at, (id, screen, _)) in listed.into_iter().enumerate() {
            let here = Some(&id) == selected.as_ref();
            if !here && at.abs_diff(at_selected) <= 1 {
                beside.push(id.clone());
            }
            entries.push(TabItem {
                id,
                name: screen.title(),
                here,
            });
        }

        if ready && (painted.seated != selected || painted.turn != turns.0) {
            painted.seated = selected.clone();
            painted.turn = turns.0;
            if let Some(id) = selected.clone() {
                commands.queue(move |world: &mut World| {
                    NativeStack::seat(id.clone(), Nav::levels::<S>(world, &id));
                    let mut wanted = Vec::new();
                    for near in beside {
                        let levels = Nav::levels::<S>(world, &near);
                        wanted.push((near, levels));
                    }
                    NativeStack::warm(wanted);
                });
            }
        }
        if painted.tabs == entries {
            return;
        }
        painted.tabs = entries.clone();
        let centre = match S::blank(1) {
            Some(_) => Some(NEW_TAB),
            None => centre.map(|centre| centre.0),
        };
        NativeStack::tabs(entries, centre);
    }

    fn declare<S: Route>(mut asked: MessageReader<Declare<S>>, mut screens: ResMut<Screens<S>>) {
        for Declare {
            name,
            component,
            presentation,
            detents,
        } in asked.read()
        {
            if screens.of(*name).is_some() {
                continue;
            }
            screens.0.push((
                *name,
                ScreenOptions {
                    component,
                    presentation: *presentation,
                    detents,
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

    fn close(mut asked: MessageReader<Close>, mut commands: Commands) {
        for Close(id) in asked.read() {
            let id = id.clone();
            commands.queue(move |world: &mut World| Nav::shut(world, &id));
        }
    }

    fn shut(world: &mut World, id: &str) {
        let mut query = world.query::<(Entity, &Tab, Option<&Local>, Option<&Selected>)>();
        let mut held = Vec::new();
        let mut found = None;
        for (entity, tab, local, selected) in query.iter(world) {
            held.push((local.map(|it| it.0), tab.id.clone()));
            if tab.id == id {
                found = Some((entity, selected.is_some()));
            }
        }
        let Some((entity, selected)) = found else {
            return;
        };
        if held.len() < 2 {
            return;
        }
        held.sort();
        let mut at = 0;
        for (index, (_, held)) in held.iter().enumerate() {
            if held == id {
                at = index;
                break;
            }
        }
        world.entity_mut(entity).despawn();
        if !selected {
            return;
        }
        let next = match held.get(at + 1) {
            Some(next) => next,
            None => &held[at - 1],
        };
        let next = next.1.clone();
        Nav::mark(world, &next);
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
            shown.push((at_tab, shows.0.clone()));
        }
        for entity in chain {
            let Some(shows) = world.get::<Shows<S>>(entity) else {
                continue;
            };
            shown.push((entity, shows.0.clone()));
        }
        let Some(screens) = world.get_resource::<Screens<S>>() else {
            return Vec::new();
        };
        let mut levels = Vec::new();
        for (_, screen) in shown {
            let Some(options) = screens.of(screen.name()) else {
                continue;
            };
            levels.push(Level {
                key: screen.key(),
                page: options.component,
                title: screen.title(),
                presentation: options.presentation,
                detents: options.detents,
                seat: Seat::taken(&screen),
            });
        }
        levels
    }

    fn open_blank<S: Route>(
        mut asked: MessageReader<OpenBlank<S>>,
        mut tapped: MessageReader<Tapped>,
        mut sprouting: MessageReader<Sprout>,
        known: Query<&Tab>,
        mut opened: ResMut<Opened>,
        mut commands: Commands,
    ) {
        let mut wanted = Vec::new();
        for OpenBlank(screen) in asked.read() {
            wanted.push((screen.clone(), Landing::Seated));
        }
        let mut at = known.iter().count();
        for Tapped(action) in tapped.read() {
            if *action != NEW_TAB {
                continue;
            }
            at += 1;
            let Some(screen) = S::blank(at) else {
                continue;
            };
            wanted.push((screen, Landing::Seated));
        }
        for Sprout in sprouting.read() {
            at += 1;
            let Some(screen) = S::blank(at) else {
                continue;
            };
            wanted.push((screen, Landing::Beside));
        }
        for (screen, landing) in wanted {
            let ordinal = opened.0;
            opened.0 = ordinal.wrapping_add(1);
            let id = format!("local:{ordinal}");
            commands.spawn((Tab { id: id.clone() }, Local(ordinal), Shows(screen)));
            if let Landing::Seated = landing {
                commands.queue(move |world: &mut World| Nav::mark(world, &id));
            }
        }
    }

    fn stack<S: Route>(
        mut pushes: MessageReader<Push<S>>,
        mut presents: MessageReader<Present<S>>,
        screens: Res<Screens<S>>,
        selected: Query<Entity, With<Selected>>,
        children: Query<&Children>,
        warming: Query<(Entity, &Shows<S>), With<Warming>>,
        mut commands: Commands,
    ) {
        let Some(tab) = selected.iter().next() else {
            return;
        };
        for Push(screen) in pushes.read() {
            let onto = Self::top(tab, &children);
            match Self::warmed(screen, &warming) {
                Some(warm) => {
                    commands
                        .entity(warm)
                        .remove::<Warming>()
                        .insert(ChildOf(onto));
                }
                None => {
                    commands.spawn((Shows(screen.clone()), ChildOf(onto)));
                }
            }
            let Some(options) = screens.of(screen.name()) else {
                continue;
            };
            NativeStack::push(Level {
                key: screen.key(),
                page: options.component,
                title: screen.title(),
                presentation: options.presentation,
                detents: options.detents,
                seat: Seat::taken(screen),
            });
        }
        for Present(screen) in presents.read() {
            let onto = Self::top(tab, &children);
            match Self::warmed(screen, &warming) {
                Some(warm) => {
                    commands
                        .entity(warm)
                        .remove::<Warming>()
                        .insert((Presented, ChildOf(onto)));
                }
                None => {
                    commands.spawn((Shows(screen.clone()), Presented, ChildOf(onto)));
                }
            }
            let Some(options) = screens.of(screen.name()) else {
                continue;
            };
            NativeStack::present(Level {
                key: screen.key(),
                page: options.component,
                title: screen.title(),
                presentation: options.presentation,
                detents: options.detents,
                seat: Seat::taken(screen),
            });
        }
    }

    fn rotate(
        mut tapped: MessageReader<Tapped>,
        selected: Query<Entity, With<Selected>>,
        children: Query<&Children>,
        presented: Query<&Presented>,
        mut turns: ResMut<Turns>,
        mut commands: Commands,
    ) {
        let mut asked = 0i32;
        for Tapped(action) in tapped.read() {
            if *action == ROTATE {
                asked += 1;
            }
            if *action == ROTATE_BACK {
                asked -= 1;
            }
        }
        if asked == 0 {
            return;
        }
        let Some(tab) = selected.iter().next() else {
            return;
        };
        let mut chain = Vec::new();
        let mut at = tab;
        while let Ok(kids) = children.get(at) {
            let Some(next) = kids.last().copied() else {
                break;
            };
            chain.push(next);
            at = next;
        }
        let mut sheets = Vec::new();
        let mut under = tab;
        for entity in chain {
            if presented.get(entity).is_ok() {
                sheets.push(entity);
                continue;
            }
            if sheets.is_empty() {
                under = entity;
            }
        }
        if sheets.len() < 2 {
            return;
        }
        for _ in 0..asked.abs() {
            if asked > 0 {
                let Some(deepest) = sheets.pop() else {
                    break;
                };
                sheets.insert(0, deepest);
            } else {
                if sheets.is_empty() {
                    break;
                }
                let front = sheets.remove(0);
                sheets.push(front);
            }
        }
        let mut parent = under;
        for entity in sheets {
            commands.entity(entity).insert(ChildOf(parent));
            parent = entity;
        }
        turns.0 = turns.0.wrapping_add(1);
    }

    fn unstack(
        mut backs: MessageReader<GoBack>,
        mut dismisses: MessageReader<Dismiss>,
        mut dropped: MessageReader<Dropped>,
        selected: Query<Entity, With<Selected>>,
        children: Query<&Children>,
        presented: Query<&Presented>,
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
            if presented.get(*level).is_ok() {
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

    #[derive(Clone, Debug, PartialEq, Hash)]
    enum Page {
        Home,
        Note(&'static str),
        Unsaved,
    }

    impl ScreenName for PageName {
        type Route = Page;
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

        fn listed(&mut self) -> Vec<String> {
            let mut ids = Vec::new();
            for tab in Nav::state::<Page>(self.0.world_mut()).tabs {
                ids.push(tab.id);
            }
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
    fn local_tabs_are_listed_in_the_order_they_were_opened() {
        let mut phone = Phone::new();
        let mut wanted = Vec::new();
        for at in 0..11 {
            phone.sends(OpenBlank(Page::Unsaved));
            wanted.push(format!("local:{at}"));
        }
        assert_eq!(phone.listed(), wanted);
    }

    #[test]
    fn a_local_tab_the_mac_never_reports_survives_every_poll() {
        let mut phone = Phone::new();
        phone.sends(OpenBlank(Page::Home));
        phone.reports(&[("tab:1", Page::Note("Unrelated"))], None);
        assert_eq!(phone.tabs(), vec!["local:0", "tab:1"]);
    }
}
