use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
#[cfg(feature = "store")]
use moonshine_save::prelude::Save;

#[cfg(feature = "store")]
use crate::{CreatedAt, Uuid};

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct Revision(pub u64);

impl Revision {
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn saturating_next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct BaseRevision(pub u64);

#[cfg(feature = "store")]
#[derive(Component, Reflect)]
#[reflect(Component)]
#[type_path = "vmux_session"]
pub struct Record;

#[cfg(feature = "store")]
impl Record {
    pub fn bundle(
        owner: Entity,
        id: Uuid,
        base: Revision,
        committed: Revision,
        at: CreatedAt,
    ) -> Result<RecordBundle, String> {
        if committed <= base {
            return Err("committed revision must advance its base".to_string());
        }
        Ok(RecordBundle {
            record: Self,
            save: Save,
            owner: ChildOf(owner),
            id,
            at,
            base: BaseRevision(base.0),
            committed,
        })
    }
}

#[cfg(feature = "store")]
#[derive(Bundle)]
pub struct RecordBundle {
    record: Record,
    save: Save,
    owner: ChildOf,
    id: Uuid,
    at: CreatedAt,
    base: BaseRevision,
    committed: Revision,
}

#[cfg(all(test, feature = "store"))]
mod tests {
    use bevy_reflect::serde::ReflectDeserializer;
    use bevy_reflect::{FromReflect, TypeRegistry};
    use serde::de::DeserializeSeed;

    use super::*;

    #[test]
    fn stored_session_revisions_keep_their_existing_reflection_format() {
        let mut registry = TypeRegistry::default();
        registry.register::<Revision>();
        registry.register::<BaseRevision>();
        let mut revision =
            ron::de::Deserializer::from_str(r#"{"vmux_session::Revision": (17)}"#).unwrap();
        let restored = ReflectDeserializer::new(&registry)
            .deserialize(&mut revision)
            .unwrap();
        assert_eq!(
            Revision::from_reflect(restored.as_ref()),
            Some(Revision(17))
        );
        let mut base =
            ron::de::Deserializer::from_str(r#"{"vmux_session::BaseRevision": (16)}"#).unwrap();
        let restored = ReflectDeserializer::new(&registry)
            .deserialize(&mut base)
            .unwrap();
        assert_eq!(
            BaseRevision::from_reflect(restored.as_ref()),
            Some(BaseRevision(16))
        );
    }
}
