use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::page_location::{PageLocationRequest, PageLocationUiState};
use vmux_api::{BinEvent, PageMetadata, PageReady, UiEventPermissions};

pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiEventPlugin::<(PageLocationRequest,)>::default(),
            crate::UiStatePlugin::<PageLocationUiState>::default(),
        ))
        .add_observer(request)
        .add_observer(ready)
        .add_systems(Update, sync);
    }
}

fn request(event: On<UiInput<PageLocationRequest>>, mut pages: Query<&mut PageMetadata>) {
    let Some(location) = event.payload.location() else {
        return;
    };
    let Ok(mut metadata) = pages.get_mut(event.webview) else {
        return;
    };
    let url = location.url(&metadata.url);
    if metadata.url != url {
        metadata.url = url;
    }
}

fn ready(
    event: On<UiInput<PageReady>>,
    pages: Query<&PageMetadata>,
    permissions: Query<&UiEventPermissions>,
    mut commands: Commands,
) {
    let Ok(metadata) = pages.get(event.webview) else {
        return;
    };
    if !UiEventPermissions::allows_page(
        permissions.iter(),
        &metadata.url,
        PageLocationUiState::id(),
    ) {
        return;
    }
    commands.trigger(crate::UiStateWrite::<PageLocationUiState>::from_event(
        event.webview,
        &PageLocationUiState::from_url(&metadata.url),
    ));
}

type Locations<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PageMetadata,
        Option<&'static crate::UiState<PageLocationUiState>>,
    ),
    Changed<PageMetadata>,
>;

fn sync(pages: Locations, permissions: Query<&UiEventPermissions>, mut commands: Commands) {
    for (entity, metadata, state) in &pages {
        if !UiEventPermissions::allows_page(
            permissions.iter(),
            &metadata.url,
            PageLocationUiState::id(),
        ) {
            continue;
        }
        let location = PageLocationUiState::from_url(&metadata.url);
        if state.and_then(crate::UiState::current) == Some(&location) {
            continue;
        }
        commands.trigger(crate::UiStateWrite::<PageLocationUiState>::from_event(
            entity, &location,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_location_request_updates_only_its_originating_page() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        let first = app
            .world_mut()
            .spawn(PageMetadata {
                url: "vmux://settings/?section=general".to_string(),
                ..Default::default()
            })
            .id();
        let second = app
            .world_mut()
            .spawn(PageMetadata {
                url: "git://repository".to_string(),
                ..Default::default()
            })
            .id();
        app.world_mut().trigger(UiInput {
            webview: first,
            payload: PageLocationRequest {
                fragment: "appearance".to_string(),
            },
        });
        assert_eq!(
            app.world().get::<PageMetadata>(first).unwrap().url,
            "vmux://settings/?section=general#appearance"
        );
        assert_eq!(
            app.world().get::<PageMetadata>(second).unwrap().url,
            "git://repository"
        );
        assert!(app.world().get::<crate::PageNavigation>(first).is_none());
    }
}
