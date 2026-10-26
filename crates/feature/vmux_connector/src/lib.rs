use bevy::prelude::*;
use vmux_feature::AppFeatureExt;

pub use link::{
    ExternalLink, ExternalLinkConflict, ExternalLinkIdentity, ExternalLinkIndex, ExternalLinkSpec,
    ExternalScopeRef, PrimaryWorkItem, ReviewArtifact, SyncPolicy, SynchronizedFields,
};
pub use scope::{CanAccess, ExternalScope, ExternalScopeIndex, ExternalScopeLookupError};
pub use value::{
    AccountExternalId, CanonicalField, ExternalHost, ExternalId, ExternalKey, ExternalNamespace,
    ExternalUrl, ProviderKey, ResourceKind,
};

mod link;
mod scope;
mod value;

pub struct ConnectorPlugin;

impl Plugin for ConnectorPlugin {
    fn build(&self, app: &mut App) {
        app.register_feature::<Feature>()
            .add_plugins((scope::ScopePlugin, link::LinkPlugin));
    }
}

struct Feature;

impl vmux_feature::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}
