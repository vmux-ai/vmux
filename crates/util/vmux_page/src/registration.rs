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

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct ModalFrame {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl ModalFrame {
    pub fn dropdown(
        anchor: vmux_api::dropdown::DropdownAnchor,
        width: f32,
        height: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<Self> {
        if !anchor.is_valid()
            || !width.is_finite()
            || !height.is_finite()
            || !viewport_width.is_finite()
            || !viewport_height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || viewport_width <= 0.0
            || viewport_height <= 0.0
        {
            return None;
        }
        let margin = 6.0;
        let gap = 4.0;
        let width = width.min((viewport_width - margin * 2.0).max(1.0));
        let height = height.min((viewport_height - margin * 2.0).max(1.0));
        let mut left = anchor.left;
        let below = anchor.top + anchor.height + gap;
        let above = anchor.top - gap - height;
        let mut top = below;
        if below + height > viewport_height - margin && above >= margin {
            top = above;
        }
        left = left.clamp(margin, (viewport_width - width - margin).max(margin));
        top = top.clamp(margin, (viewport_height - height - margin).max(margin));
        Some(Self {
            left,
            top,
            width,
            height,
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(left: f32, top: f32) -> vmux_api::dropdown::DropdownAnchor {
        vmux_api::dropdown::DropdownAnchor {
            left,
            top,
            width: 100.0,
            height: 32.0,
        }
    }

    #[test]
    fn dropdown_frame_opens_below_its_anchor() {
        let frame = ModalFrame::dropdown(anchor(24.0, 40.0), 220.0, 180.0, 800.0, 600.0).unwrap();

        assert_eq!(
            frame,
            ModalFrame {
                left: 24.0,
                top: 76.0,
                width: 220.0,
                height: 180.0,
            }
        );
    }

    #[test]
    fn dropdown_frame_flips_above_when_below_does_not_fit() {
        let frame = ModalFrame::dropdown(anchor(24.0, 540.0), 220.0, 180.0, 800.0, 600.0).unwrap();

        assert_eq!(frame.top, 356.0);
    }

    #[test]
    fn dropdown_frame_stays_inside_the_viewport() {
        let frame = ModalFrame::dropdown(anchor(780.0, 590.0), 900.0, 900.0, 800.0, 600.0).unwrap();

        assert_eq!(
            frame,
            ModalFrame {
                left: 6.0,
                top: 6.0,
                width: 788.0,
                height: 588.0,
            }
        );
    }
}
