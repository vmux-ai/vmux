use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
#[cfg(all(host, feature = "page-binding"))]
use vmux_ecs::page::{PageManifest, PagePlacement, PageReuse, PageSplitAxis, PageSplitPreference};

use crate::FeaturePageBinding;
use crate::manifest::{FeatureManifestSource, Page};

type BindingKey = (TypeId, &'static str);
type ResolvedCell = OnceLock<ResolvedPage>;
type BindingCache = Mutex<HashMap<BindingKey, &'static ResolvedCell>>;

impl<M: FeatureManifestSource> FeaturePageBinding<M> {
    fn resolved(&self) -> Result<&'static ResolvedPage, String> {
        static PAGES: OnceLock<BindingCache> = OnceLock::new();
        let declaration = self.declaration()?;
        let cell = {
            let mut pages = PAGES
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .expect("page binding cache");
            let key = (TypeId::of::<M>(), declaration.url.as_str());
            if let Some(cell) = pages.get(&key) {
                *cell
            } else {
                let cell: &'static ResolvedCell = Box::leak(Box::new(OnceLock::new()));
                pages.insert(key, cell);
                cell
            }
        };
        Ok(cell.get_or_init(|| ResolvedPage::of(declaration)))
    }

    pub fn permissions(&self) -> Result<&'static [&'static str], String> {
        Ok(self.resolved()?.permissions)
    }

    #[cfg(feature = "page-binding")]
    pub fn page(
        &self,
        mut page: vmux_page::Page,
        states: &[&str],
    ) -> Result<vmux_page::Page, String> {
        self.validate(states)?;
        let resolved = self.resolved()?;
        let declaration = resolved.declaration;
        page.url = &declaration.url;
        page.title = &declaration.title;
        page.permissions = resolved.permissions;
        page.owns_subtree |= declaration.owns_subtree;
        if page.document_url.is_none() && !declaration.manifest_url.is_empty() {
            page.document_url = Some(&declaration.manifest_url);
        }
        Ok(page)
    }

    #[cfg(all(host, feature = "page-binding"))]
    pub fn manifest(&self) -> Result<&'static PageManifest, String> {
        let resolved = self.resolved()?;
        match resolved.host.get_or_init(|| resolved.manifest()) {
            Ok(manifest) => Ok(manifest),
            Err(error) => Err(error.clone()),
        }
    }
}

struct ResolvedPage {
    #[cfg(feature = "page-binding")]
    declaration: &'static Page,
    permissions: &'static [&'static str],
    #[cfg(all(host, feature = "page-binding"))]
    keywords: &'static [&'static str],
    #[cfg(all(host, feature = "page-binding"))]
    host: OnceLock<Result<PageManifest, String>>,
}

impl ResolvedPage {
    fn of(declaration: &'static Page) -> Self {
        let mut permissions = Vec::new();
        for permission in &declaration.permissions {
            permissions.push(permission.as_str());
        }
        #[cfg(all(host, feature = "page-binding"))]
        let keywords = {
            let mut keywords = Vec::new();
            for keyword in &declaration.keywords {
                keywords.push(keyword.as_str());
            }
            Box::leak(keywords.into_boxed_slice())
        };
        Self {
            #[cfg(feature = "page-binding")]
            declaration,
            permissions: Box::leak(permissions.into_boxed_slice()),
            #[cfg(all(host, feature = "page-binding"))]
            keywords,
            #[cfg(all(host, feature = "page-binding"))]
            host: OnceLock::new(),
        }
    }

    #[cfg(all(host, feature = "page-binding"))]
    fn manifest(&self) -> Result<PageManifest, String> {
        let page = self.declaration;
        let url = if page.manifest_url.is_empty() {
            page.url.as_str()
        } else {
            page.manifest_url.as_str()
        };
        let title = if page.manifest_title.is_empty() {
            page.title.as_str()
        } else {
            page.manifest_title.as_str()
        };
        let asset_host = page.asset_host()?;
        let icon = page.builtin_icon()?;
        let placement = match &page.placement {
            None => PagePlacement::DEFAULT,
            Some(placement) => {
                let reuse = match placement.reuse {
                    crate::manifest::PageReuse::Exact => PageReuse::Exact,
                    crate::manifest::PageReuse::Group => PageReuse::Group,
                    crate::manifest::PageReuse::Route => PageReuse::Route,
                    crate::manifest::PageReuse::Fragmentless => PageReuse::Fragmentless,
                };
                let split = match &placement.split {
                    None => None,
                    Some(split) => {
                        let mut allowed = Vec::new();
                        for group in &split.allowed_groups {
                            allowed.push(group.as_str());
                        }
                        let axis = match split.axis {
                            crate::manifest::PageSplitAxis::Row => PageSplitAxis::Row,
                            crate::manifest::PageSplitAxis::Column => PageSplitAxis::Column,
                        };
                        Some(PageSplitPreference {
                            anchor_group: &split.anchor_group,
                            allowed_groups: Box::leak(allowed.into_boxed_slice()),
                            axis,
                        })
                    }
                };
                PagePlacement {
                    group: &placement.group,
                    auxiliary: placement.auxiliary,
                    refresh: placement.refresh,
                    reuse,
                    split,
                }
            }
        };
        Ok(PageManifest {
            route: &page.url,
            url,
            asset_host,
            owns_subtree: page.owns_subtree,
            title,
            title_message_id: if page.title_message_id.is_empty() {
                None
            } else {
                Some(&page.title_message_id)
            },
            replaces_command: if page.replaces_command.is_empty() {
                None
            } else {
                Some(&page.replaces_command)
            },
            keywords: self.keywords,
            icon,
            command_bar: page.command_bar,
            startup: page.startup,
            reports_title: page.reports_title,
            placement,
        })
    }
}

#[cfg(all(test, host, feature = "page"))]
mod tests {
    use super::*;

    struct Feature;

    impl FeatureManifestSource for Feature {
        const SOURCE: &'static str = r#"(pages: [(
            name: "main", url: "vmux://logical/", title: "Logical",
            manifest_url: "vmux://document/", manifest_title: "Document", asset_host: "document",
            owns_subtree: true, permissions: ["Allowed"], keywords: ["find"], icon: "Code",
            placement: Some((group: "files", reuse: Fragmentless, split: Some((
                anchor_group: "files", allowed_groups: ["files", "terminal"], axis: Column,
            )))),
        )])"#;
    }

    #[test]
    fn cached_metadata_preserves_logical_aliases_permissions_and_placement_behavior() {
        let binding = FeaturePageBinding::<Feature>::new(Some("main"));
        let manifest = binding.manifest().unwrap();
        assert!(std::ptr::eq(manifest, binding.manifest().unwrap()));
        let permissions = binding.permissions().unwrap();
        assert!(std::ptr::eq(permissions, binding.permissions().unwrap()));
        assert!(manifest.routes("vmux://logical/item"));
        assert!(manifest.routes("vmux://document/item"));
        assert!(!manifest.routes("vmux://other/item"));
        assert!(manifest.placement.reuses(
            "file:///one#new",
            manifest.placement,
            "file:///one#old"
        ));
        assert!(
            !manifest
                .placement
                .reuses("file:///one", manifest.placement, "file:///two")
        );
        assert_eq!(
            manifest.placement.split.unwrap().allowed_groups,
            ["files", "terminal"]
        );
        assert_eq!(manifest.icon, Some(vmux_api::BuiltinIcon::Code));
        binding.validate(&["Allowed"]).unwrap();
        assert!(binding.validate(&["Denied"]).is_err());
    }
}
