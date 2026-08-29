use dioxus::prelude::*;

use crate::nav::{
    Declare, Dismiss, GoBack, Nav, NavigationState, Present, Push, Route, ScreenName, ScreenPage,
    Screens, Seat, Select,
};
use crate::runtime::World;

pub struct Router<R: Route> {
    state: Signal<NavigationState<R>>,
    here: Signal<Option<R>>,
}

impl<R: Route> Clone for Router<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Route> Copy for Router<R> {}

impl<R: Route> PartialEq for Router<R> {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state && self.here == other.here
    }
}

impl<R: Route> Router<R> {
    pub fn route(&self) -> Option<R> {
        self.here.read().clone()
    }

    pub fn top(&self) -> Option<R> {
        self.state.read().current.clone()
    }

    pub fn state(&self) -> NavigationState<R> {
        self.state.read().clone()
    }

    pub fn segments(&self) -> Vec<R> {
        self.state.read().trail.clone()
    }

    pub fn depth(&self) -> usize {
        self.state.read().depth
    }

    pub fn position(&self) -> usize {
        let here = self.here.read();
        let state = self.state.read();
        let mut at = state.depth;
        for (index, route) in state.trail.iter().enumerate() {
            if Some(route) == here.as_ref() {
                at = index;
            }
        }
        at
    }

    pub fn attached<C: bevy_ecs::prelude::Component + Clone>(&self) -> Option<C> {
        let here = self.here.read().clone()?;
        World::with(|world| {
            world.read(|world| {
                let mut screens = world.query::<(&crate::nav::Shows<R>, &C)>();
                for (shows, found) in screens.iter(world) {
                    if shows.0 == here {
                        return Some(found.clone());
                    }
                }
                None
            })
        })?
    }

    pub fn pathname(&self) -> String {
        let mut crumbs = Vec::new();
        for route in self.state.read().trail.iter() {
            crumbs.push(route.title());
        }
        crumbs.join(" \u{203a} ")
    }

    pub fn push(&self, route: R) {
        let name = route.name();
        World::with(|world| {
            let pushes = world
                .read(|world| {
                    let declared = world.get_resource::<Screens<R>>()?;
                    Some(declared.of(name)?.presentation.pushes())
                })
                .unwrap_or(true);
            if pushes {
                world.send(Push(route));
            } else {
                world.send(Present(route));
            }
        });
    }

    pub fn back(&self) {
        let sheet = self.state.read().sheet;
        World::with(|world| {
            if sheet {
                world.send(Dismiss);
            } else {
                world.send(GoBack);
            }
        });
    }

    pub fn dismiss(&self) {
        World::with(|world| world.send(Dismiss));
    }

    pub fn can_go_back(&self) -> bool {
        self.state.read().depth > 0
    }

    pub fn navigate(&self, tab: &str) {
        let tab = tab.to_string();
        World::with(|world| world.send(Select(tab)));
    }
}

pub fn use_router<R: Route>() -> Router<R> {
    let mut state =
        use_signal(|| World::with(|world| world.read(Nav::state::<R>)).unwrap_or_default());
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            let Some(seen) = World::with(|world| world.read(Nav::state::<R>)) else {
                continue;
            };
            if *state.peek() != seen {
                state.set(seen);
            }
        }
    });
    let seen = use_route::<R>();
    let here = use_signal(move || seen);
    Router { state, here }
}

pub fn use_route<R: Route>() -> Option<R> {
    use_hook(try_consume_context::<Seat<R>>).map(|seat| seat.0)
}

#[component]
pub fn Stack(children: Element) -> Element {
    rsx! {
        {children}
    }
}

#[component]
pub fn Tabs(children: Element) -> Element {
    rsx! {
        {children}
    }
}

#[component]
pub fn Screen<N: ScreenName>(page: &'static ScreenPage<N>) -> Element {
    use_effect(move || {
        World::with(|world| {
            world.send(Declare::<N::Route> {
                name: page.name,
                component: page.page,
                presentation: page.presentation,
                detents: page.detents,
            })
        });
    });
    rsx! {}
}
