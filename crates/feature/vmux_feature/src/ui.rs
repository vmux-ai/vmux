use dioxus::prelude::Element;
use dioxus::prelude::*;
use vmux_api::address::{AddressOption, PageAddress};
use vmux_ui::components::manager::{
    ManagerButton, ManagerButtonVariant, ManagerDetails, ManagerEmpty, ManagerList, ManagerPage,
    ManagerRow, ManagerSearch, ManagerSearchRow, ManagerSelect, ManagerSelectItem,
    ManagerSidebarItem, ManagerSkeleton, WorkspaceLayout,
};
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

use crate::state::*;

#[vmux_page::page(component=Page,states=["ThemeUiState","PageAddressUiState","FeaturesUiState"],subtree)]
pub(crate) struct FeaturesPage;

#[component]
fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<FeaturesUiState>().state;
    let state = state();
    rsx! {
        document::Title { {translate("features-title")} }
        ManagerPage {
            PageBreadcrumbs { state:PageAddress {
                current:AddressOption { label_key:"features-title".into(),icon:Some(vmux_api::BuiltinIcon::Settings),..Default::default() },
                ..Default::default()
            } }
            WorkspaceLayout {
                search:rsx! { ManagerSearchRow {
                    ManagerSearch { value:state.query.clone(),placeholder:translate("features-search"),
                        onsearch:move |query| { let _=send(&FeaturesFilterRequest {query}); } }
                } },
                sidebar:rsx! { FeaturesSidebar { state:state.clone() } },
                toolbar:rsx! { FeaturesToolbar { state:state.clone() } },
                FeaturesBody { state }
            }
        }
    }
}

#[component]
fn FeaturesSidebar(state: FeaturesUiState) -> Element {
    let key = state.selected.clone();
    let changes = state.flags.iter().filter(|flag| flag.changed).count();
    rsx! {
        ManagerSidebarItem { label:translate("features-all"),count:state.flags.len(),selected:state.tab=="overview" && state.selected.is_empty(),
            onclick:move |_| { let _=send(&FeatureSelectRequest {key:String::new(),tab:"overview".into()}); } }
        ManagerSidebarItem { label:translate("features-changes"),count:changes,selected:state.tab=="changes",
            onclick:move |_| { let _=send(&FeatureSelectRequest {key:key.clone(),tab:"changes".into()}); } }
        ManagerSidebarItem { label:translate("features-history"),count:state.history.len(),selected:state.tab=="history",
            onclick:move |_| { let _=send(&FeatureSelectRequest {key:String::new(),tab:"history".into()}); } }
    }
}

#[component]
fn FeaturesToolbar(state: FeaturesUiState) -> Element {
    let items = state
        .environments
        .iter()
        .map(|environment| ManagerSelectItem {
            value: environment.clone(),
            label: environment.clone(),
        })
        .collect();
    let revision_text = state.revision.to_string();
    let revision = translate_with(
        "features-revision",
        &[("revision", TranslationValue::String(&revision_text))],
    );
    rsx! {
        div { class:"flex min-w-0 items-center gap-3",
            ManagerSelect { items,value:Some(state.environment.clone()),placeholder:translate("features-environment"),disabled:state.busy,
                onselect:move |environment| { let _=send(&FeaturesOpenRequest {environment}); } }
            span { class:"text-xs text-muted-foreground", "{revision}" }
            if state.loaded && !state.can_edit { span { class:"text-xs text-muted-foreground", {translate("features-read-only")} } }
            PageToolbar { controls:vec![ToolbarControl::button(translate("common-refresh"),move |_| { let _=send(&FeaturesOpenRequest::default()); }).disabled(state.busy)] }
        }
    }
}

#[component]
fn FeaturesBody(state: FeaturesUiState) -> Element {
    let mut visible = Vec::new();
    let query = state.query.to_lowercase();
    for flag in &state.flags {
        if flag.key.to_lowercase().contains(&query) {
            visible.push(flag.clone());
        }
    }
    let selected = state
        .flags
        .iter()
        .find(|flag| flag.key == state.selected)
        .cloned();
    rsx! {
        div { class:"flex min-h-0 flex-1 flex-col",
            if !state.error.is_empty() {
                div { class:"px-5 py-3 text-xs text-ansi-1", role:"alert",
                    if state.error.starts_with("features-") { {translate(&state.error)} } else { "{state.error}" }
                }
            }
            if !state.loaded {
                ManagerList { ManagerSkeleton {} }
            } else if state.tab=="changes" {
                FeaturesReview { state:state.clone() }
            } else if state.tab=="history" {
                FeaturesHistory { state:state.clone() }
            } else if visible.is_empty() {
                ManagerEmpty { title:translate("common-no-results"),detail:translate("features-empty") }
            } else {
                div { class:"flex min-h-0 flex-1",
                    div { class:"min-w-0 flex-1 overflow-y-auto",
                        ManagerList {
                            for flag in visible {
                                FeatureRow { key:"{flag.key}",selected:flag.key==state.selected,flag }
                            }
                        }
                    }
                    if let Some(flag)=selected { FeatureDetails { flag,state:state.clone() } }
                }
                if !state.draft_id.is_empty() { FeaturesActions { state:state.clone() } }
            }
        }
    }
}

#[component]
fn FeatureRow(flag: FlagRow, selected: bool) -> Element {
    let key = flag.key.clone();
    rsx! {
        ManagerRow { title:flag.key.clone(),subtitle:translate("features-boolean"),show_icon:false,icon:rsx! {},selected,
            meta:rsx! { div { class:"flex items-center gap-2 text-xs text-muted-foreground",
                FeatureValue { value:flag.published }
                if flag.changed { span { {translate("features-draft")} } }
            } },
            actions:rsx! {},
            onselect:move |_| { let _=send(&FeatureSelectRequest {key:key.clone(),tab:"overview".into()}); }
        }
    }
}

#[component]
fn FeatureDetails(flag: FlagRow, state: FeaturesUiState) -> Element {
    let key = flag.key.clone();
    rsx! {
        ManagerDetails { title:flag.key.clone(),subtitle:translate("features-overview"),show_icon:false,icon:rsx! {},actions:rsx! {},
            onclose:move |_| { let _=send(&FeatureSelectRequest {key:String::new(),tab:"overview".into()}); },
            div { class:"flex flex-col gap-5 px-4 pb-5 text-xs",
                FeatureField { label:translate("features-published"), FeatureValue {value:flag.published} }
                FeatureField { label:translate("features-default"),
                    if flag.default.is_some() {FeatureValue {value:flag.default}} else {{translate("features-not-declared")}}
                }
                FeatureField { label:translate("features-source"), {translate(if flag.source=="default" {"features-default"} else {"features-published"})} }
                FeatureField { label:translate("features-draft"),
                    div { class:"flex items-center gap-3",
                        Switch { checked:flag.proposed,disabled:state.busy || !state.can_edit,
                            on_checked_change:move |enabled| { let _=send(&DraftEditRequest {key:key.clone(),enabled}); }, SwitchThumb { attributes:vec![] } }
                        FeatureValue {value:Some(flag.proposed)}
                    }
                }
                p { class:"text-muted-foreground", {translate("features-draft-help")} }
            }
        }
    }
}

#[component]
fn FeatureField(label: String, children: Element) -> Element {
    rsx! { div { class:"flex flex-col gap-2", div {class:"font-medium text-muted-foreground","{label}"} {children} } }
}

#[component]
fn FeatureValue(value: Option<bool>) -> Element {
    rsx! { span { {translate(match value {Some(true)=>"features-on",Some(false)=>"features-off",None=>"features-not-published"})} } }
}

#[component]
fn FeaturesReview(state: FeaturesUiState) -> Element {
    let changes: Vec<_> = state
        .flags
        .iter()
        .filter(|flag| flag.changed)
        .cloned()
        .collect();
    let revision_text = state.base_revision.to_string();
    let summary = translate_with(
        "features-review-summary",
        &[
            ("environment", TranslationValue::String(&state.environment)),
            ("revision", TranslationValue::String(&revision_text)),
        ],
    );
    rsx! {
        div { class:"flex min-h-0 flex-1 flex-col",
            div { class:"px-5 py-4",
                h2 {class:"text-sm font-semibold",{translate("features-review")}}
                p {class:"mt-2 text-xs text-muted-foreground","{summary}"}
                if state.reviewed { p {class:"mt-2 text-xs text-success",{translate("features-reviewed")}} }
                if state.unsaved {p {class:"mt-2 text-xs text-muted-foreground",{translate("features-unsaved")}}}
            }
            if changes.is_empty() {
                ManagerEmpty {title:translate("features-no-changes"),detail:translate("features-draft-help")}
            } else {
                div {class:"min-h-0 flex-1 overflow-y-auto",
                    ManagerList {
                        for flag in changes {
                            ManagerRow {key:"{flag.key}",title:flag.key,subtitle:String::new(),show_icon:false,icon:rsx! {},actions:rsx! {},
                                meta:rsx! {div {class:"flex items-center gap-3 text-xs",FeatureValue {value:flag.published} span {"→"} FeatureValue {value:Some(flag.proposed)}}} }
                        }
                    }
                }
            }
            if !state.draft_id.is_empty() { FeaturesActions {state} }
        }
    }
}

#[component]
fn FeaturesActions(state: FeaturesUiState) -> Element {
    let draft = state.draft_id.clone();
    let review_draft = draft.clone();
    let publish_draft = draft.clone();
    let expected = state.draft_revision;
    let base = state.base_revision;
    let in_review = state.tab == "changes";
    let review_key = state.selected.clone();
    rsx! {
        div {class:"flex items-center justify-end gap-2 px-5 py-4",
            ManagerButton {variant:ManagerButtonVariant::Ghost,disabled:state.busy || !state.can_edit,
                onclick:move |_| {let _=send(&DraftDiscardRequest {draft_id:draft.clone(),expected_revision:expected});}, {translate("features-discard")} }
            ManagerButton {variant:ManagerButtonVariant::Secondary,disabled:state.busy || !state.can_edit || !state.unsaved,
                onclick:move |_| {let _=send(&DraftSaveRequest {expected_revision:expected});}, {translate("common-save")} }
            ManagerButton {variant:ManagerButtonVariant::Secondary,disabled:state.busy || (in_review && (!state.can_review || state.unsaved || expected==0 || state.reviewed)),
                onclick:move |_| {
                    if in_review {let _=send(&DraftReviewRequest {draft_id:review_draft.clone(),expected_revision:expected,base_revision:base});}
                    else {let _=send(&FeatureSelectRequest {key:review_key.clone(),tab:"changes".into()});}
                }, {translate(if in_review {"features-mark-reviewed"} else {"features-review"})} }
            ManagerButton {disabled:state.busy || !state.can_publish || state.unsaved || !state.reviewed || !in_review || state.base_revision!=state.revision || expected==0,
                onclick:move |_| {let _=send(&DraftPublishRequest {draft_id:publish_draft.clone(),expected_revision:expected,base_revision:base});}, {translate("features-publish")} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ui::i18n::Locale;

    struct Fixture {
        previous: Locale,
    }

    impl Fixture {
        fn new() -> Self {
            let previous = Locale::current();
            Locale::from("en-US").make_current();
            Self { previous }
        }

        fn actions(&self, state: FeaturesUiState) -> String {
            let mut dom =
                VirtualDom::new_with_props(FeaturesActions, FeaturesActionsProps { state });
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        }

        fn publish_disabled(&self, html: &str) -> bool {
            for part in html.split("</button>") {
                let Some(start) = part.rfind("<button") else {
                    continue;
                };
                let button = &part[start..];
                if !button.contains(&translate("features-publish")) {
                    continue;
                }
                let end = button.find('>').expect("button opening tag");
                return button[..end].split_whitespace().any(|attribute| {
                    attribute == "disabled" || attribute.starts_with("disabled=")
                });
            }
            panic!("Publish action was not rendered");
        }

        fn review(&self, state: FeaturesUiState) -> String {
            let mut dom = VirtualDom::new_with_props(FeaturesReview, FeaturesReviewProps { state });
            dom.rebuild_in_place();
            dioxus_ssr::render(&dom)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.previous.make_current();
        }
    }

    #[test]
    fn publish_is_disabled_until_the_current_reviewed_change_set_is_ready() {
        let fixture = Fixture::new();
        for (reviewed, unsaved, busy, allowed, base, current, tab, disabled) in [
            (false, false, false, true, 10, 10, "changes", true),
            (true, true, false, true, 10, 10, "changes", true),
            (true, false, true, true, 10, 10, "changes", true),
            (true, false, false, false, 10, 10, "changes", true),
            (true, false, false, true, 10, 11, "changes", true),
            (true, false, false, true, 10, 10, "overview", true),
            (true, false, false, true, 10, 10, "changes", false),
        ] {
            let state = FeaturesUiState {
                reviewed,
                unsaved,
                busy,
                can_publish: allowed,
                base_revision: base,
                revision: current,
                draft_id: "draft".into(),
                draft_revision: 1,
                tab: tab.into(),
                ..Default::default()
            };
            assert_eq!(fixture.publish_disabled(&fixture.actions(state)), disabled);
        }
    }

    #[test]
    fn review_shows_the_entire_change_set_even_when_search_filters_the_list() {
        let fixture = Fixture::new();
        let state = FeaturesUiState {
            query: "visible".into(),
            flags: vec![
                FlagRow {
                    key: "visible.flag".into(),
                    published: Some(false),
                    proposed: true,
                    changed: true,
                    ..Default::default()
                },
                FlagRow {
                    key: "other.flag".into(),
                    published: Some(true),
                    proposed: false,
                    changed: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let html = fixture.review(state);
        assert!(html.contains("visible.flag"));
        assert!(html.contains("other.flag"));
        assert!(html.contains(&translate("features-on")));
        assert!(html.contains(&translate("features-off")));
    }
}

#[component]
fn FeaturesHistory(state: FeaturesUiState) -> Element {
    rsx! {
        div {class:"min-h-0 flex-1 overflow-y-auto",
            if state.history.is_empty() {ManagerEmpty {title:translate("features-no-history"),detail:String::new()}}
            else {
                ManagerList {
                    for entry in state.history.iter().rev() {
                        {
                            let revision=entry.revision;
                            let revision_text=revision.to_string();
                            let title=translate_with("features-revision",&[("revision",TranslationValue::String(&revision_text))]);
                            let count=translate_with("features-history-changes",&[("count",TranslationValue::Number(entry.changes.len() as i64))]);
                            let timestamp=i64::try_from(entry.created_at).ok().and_then(|seconds| chrono::DateTime::from_timestamp(seconds,0));
                            let timestamp=timestamp.map(|date| date.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_else(|| "—".into());
                            let subtitle=format!("{} · {}",entry.actor,timestamp);
                            rsx! {ManagerRow {key:"{revision}",title,subtitle,show_icon:false,icon:rsx! {},
                                meta:rsx! {span {class:"text-xs text-muted-foreground","{count}"}},
                                actions:rsx! {ManagerButton {variant:ManagerButtonVariant::Secondary,disabled:state.busy || !state.can_edit || !state.draft_id.is_empty() || revision==state.revision,
                                    onclick:move |_| {let _=send(&FlagRollbackRequest {revision});}, {translate("features-rollback")} }} }
                            }
                        }
                    }
                }
            }
        }
    }
}
