use dioxus::prelude::*;

use crate::nav::{Dismiss, Dropped, GoBack, Nav, Present, Push, Route, Select, View};
use crate::runtime::World;
use crate::transition;

pub struct Navigation<R: Route> {
    view: Signal<View<R>>,
}

impl<R: Route> Clone for Navigation<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Route> Copy for Navigation<R> {}

impl<R: Route> PartialEq for Navigation<R> {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
    }
}

impl<R: Route> Navigation<R> {
    pub fn route(&self) -> Option<R> {
        self.view.read().current.clone()
    }

    pub fn view(&self) -> View<R> {
        self.view.read().clone()
    }

    pub fn push(&self, route: R) {
        World::with(|world| world.send(Push(route)));
    }

    pub fn present(&self, route: R) {
        World::with(|world| world.send(Present(route)));
    }

    pub fn go_back(&self) {
        let sheet = self.view.read().sheet;
        World::with(|world| {
            if sheet {
                world.send(Dismiss);
            } else {
                world.send(GoBack);
            }
        });
    }

    pub fn navigate(&self, tab: &str) {
        let tab = tab.to_string();
        World::with(|world| world.send(Select(tab)));
    }
}

pub fn use_navigation<R: Route>() -> Navigation<R> {
    use_context()
}

#[component]
pub fn NavigationContainer<R: Route>(
    #[props(default)] route: std::marker::PhantomData<R>,
    children: Element,
) -> Element {
    let _ = route;
    let mut view = use_signal(View::<R>::default);
    use_context_provider(|| Navigation { view });

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            let dropped = transition::take_popped() + transition::take_dismissed();
            if dropped > 0 {
                World::with(|world| world.send(Dropped(dropped)));
            }
            let Some(seen) = World::with(|world| world.read(Nav::view::<R>)) else {
                continue;
            };
            if *view.peek() != seen {
                view.set(seen);
            }
        }
    });

    rsx! {
        {children}
    }
}

#[component]
pub fn TabNavigator(children: Element) -> Element {
    rsx! {
        {children}
    }
}

#[component]
pub fn Screen<R: Route>(name: R::Name, children: Element) -> Element {
    let navigation = use_navigation::<R>();
    let Some(route) = navigation.route() else {
        return rsx! {};
    };
    if route.name() != name {
        return rsx! {};
    }
    rsx! {
        {children}
    }
}
