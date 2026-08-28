use dioxus::prelude::*;

use crate::nav::{
    Declare, Declared, Dismiss, GoBack, Nav, Present, Presentation, Push, Route, Seat, Select, View,
};
use crate::runtime::World;

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

    pub fn go(&self, route: R) {
        let name = route.name();
        World::with(|world| {
            let pushes = world
                .read(|world| {
                    let declared = world.get_resource::<Declared<R>>()?;
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

pub fn use_route<R: Route>() -> Option<R> {
    use_hook(try_consume_context::<Seat<R>>).map(|seat| seat.0)
}

#[component]
pub fn Stack<R: Route>(
    #[props(default)] route: std::marker::PhantomData<R>,
    children: Element,
) -> Element {
    let _ = route;
    let mut view =
        use_signal(|| World::with(|world| world.read(Nav::view::<R>)).unwrap_or_default());
    use_context_provider(|| Navigation { view });

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
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
pub fn Tabs(children: Element) -> Element {
    rsx! {
        {children}
    }
}

#[component]
pub fn Screen<R: Route>(
    name: R::Name,
    draws: &'static vmux_native::NativePage,
    #[props(default = Presentation::Card)] presentation: Presentation,
    #[props(default = &[])] detents: &'static [f64],
    #[props(default)] action: Option<&'static str>,
) -> Element {
    use_effect(move || {
        World::with(|world| {
            world.send(Declare::<R> {
                name,
                draws,
                presentation,
                detents,
                action,
            })
        });
    });
    rsx! {}
}
