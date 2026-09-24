use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;

use crate::{Instance, NativePage, PageScope};

type ReadInstance = fn(&World, Entity) -> Instance;
type ClaimsPage = fn(&World, Entity) -> bool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePagePlacement {
    Layout,
    Pane,
    Modal,
}

#[derive(Component, Clone, Copy)]
pub struct NativePageRegistration {
    page: &'static NativePage,
    placement: NativePagePlacement,
    instance: Option<ReadInstance>,
    claim: Option<ClaimsPage>,
}

impl NativePageRegistration {
    pub fn page(self) -> &'static NativePage {
        self.page
    }

    pub fn placement(self) -> NativePagePlacement {
        self.placement
    }

    pub fn answers_for(self, world: &World, entity: Entity, url: &str) -> bool {
        match self.claim {
            Some(claim) => claim(world, entity),
            None => self.page.answers_for(url),
        }
    }

    pub fn instance(self, world: &World, entity: Entity) -> Instance {
        match self.instance {
            Some(read) => read(world, entity),
            None => Instance::default(),
        }
    }
}

pub struct NativePagePlugin {
    registration: NativePageRegistration,
}

impl Plugin for NativePagePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(self.registration);
    }

    fn is_unique(&self) -> bool {
        false
    }
}

impl NativePagePlugin {
    pub fn in_pane(page: &'static NativePage) -> Self {
        Self::new(page, NativePagePlacement::Pane)
    }

    pub fn as_layout(page: &'static NativePage) -> Self {
        Self::new(page, NativePagePlacement::Layout)
    }

    pub fn as_modal(page: &'static NativePage) -> Self {
        Self::new(page, NativePagePlacement::Modal)
    }

    pub fn takes<C: Component + Clone>(mut self) -> Self {
        self.registration.instance = Some(Self::read::<C>);
        self
    }

    pub fn claims<C: Component>(mut self) -> Self {
        self.registration.claim = Some(Self::has::<C>);
        self
    }

    fn new(page: &'static NativePage, placement: NativePagePlacement) -> Self {
        Self {
            registration: NativePageRegistration {
                page,
                placement,
                instance: None,
                claim: None,
            },
        }
    }

    fn read<C: Component + Clone>(world: &World, entity: Entity) -> Instance {
        let Some(value) = world.get::<C>(entity).cloned() else {
            return Instance::default();
        };
        Instance::from(move |scope: PageScope<'_>| scope.provide(value))
    }

    fn has<C: Component>(world: &World, entity: Entity) -> bool {
        world.get::<C>(entity).is_some()
    }
}
