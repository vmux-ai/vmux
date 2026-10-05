use bevy_app::{App, Plugin, PreStartup};
use bevy_ecs::prelude::*;
use bevy_ecs::world::EntityRef;

use crate::{Instance, Page, PageScope};

type ReadInstance = for<'w> fn(EntityRef<'w>) -> Instance;
type ClaimsPage = for<'w> fn(EntityRef<'w>) -> bool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagePlacement {
    Layout,
    Pane,
    Modal,
}

#[derive(Component, Clone, Copy)]
pub struct PageRegistration {
    page: &'static Page,
    placement: PagePlacement,
    instance: Option<ReadInstance>,
    claim: Option<ClaimsPage>,
}

impl PageRegistration {
    pub fn page(self) -> &'static Page {
        self.page
    }

    pub fn placement(self) -> PagePlacement {
        self.placement
    }

    pub fn answers_for(self, entity: EntityRef<'_>, url: &str) -> bool {
        match self.claim {
            Some(claim) => claim(entity),
            None => self.page.answers_for(url),
        }
    }

    pub fn instance(self, entity: EntityRef<'_>) -> Instance {
        match self.instance {
            Some(read) => read(entity),
            None => Instance::default(),
        }
    }
}

pub struct PagePlugin {
    registration: PageRegistration,
}

impl Plugin for PagePlugin {
    fn build(&self, app: &mut App) {
        let registration = self.registration;
        app.add_systems(PreStartup, move |mut commands: Commands| {
            commands.spawn((registration, registration.page.page_permissions()));
        });
    }

    fn is_unique(&self) -> bool {
        false
    }
}

impl PagePlugin {
    pub fn in_pane(page: &'static Page) -> Self {
        Self::new(page, PagePlacement::Pane)
    }

    pub fn as_layout(page: &'static Page) -> Self {
        Self::new(page, PagePlacement::Layout)
    }

    pub fn as_modal(page: &'static Page) -> Self {
        Self::new(page, PagePlacement::Modal)
    }

    pub fn takes<C: Component + Clone>(mut self) -> Self {
        self.registration.instance = Some(Self::read::<C>);
        self
    }

    pub fn claims<C: Component>(mut self) -> Self {
        self.registration.claim = Some(Self::has::<C>);
        self
    }

    fn new(page: &'static Page, placement: PagePlacement) -> Self {
        Self {
            registration: PageRegistration {
                page,
                placement,
                instance: None,
                claim: None,
            },
        }
    }

    fn read<C: Component + Clone>(entity: EntityRef<'_>) -> Instance {
        let Some(value) = entity.get::<C>().cloned() else {
            return Instance::default();
        };
        Instance::from(move |scope: PageScope<'_>| scope.provide(value))
    }

    fn has<C: Component>(entity: EntityRef<'_>) -> bool {
        entity.contains::<C>()
    }
}
