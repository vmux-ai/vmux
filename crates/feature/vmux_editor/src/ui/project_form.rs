use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::hooks::{send, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::project::{
    ProjectEntry, ProjectFolderPickRequest, ProjectFormUiState, ProjectSaveRequest, ProjectSource,
};

#[component]
pub(super) fn ProjectEditor(entry: Option<ProjectEntry>, onclose: EventHandler<()>) -> Element {
    let editing = entry.is_some();
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| { if !open { onclose.call(()); } },
            attributes: vec![],
            DialogContent { class: "max-w-[460px] p-4", attributes: vec![],
                DialogTitle { attributes: vec![], {translate(if editing { "project-edit" } else { "project-new" })} }
                ProjectForm { entry, onclose }
            }
        }
    }
}

#[component]
fn ProjectForm(entry: Option<ProjectEntry>, onclose: EventHandler<()>) -> Element {
    let request_id = use_signal(|| uuid::Uuid::new_v4().to_string());
    let mut name = use_signal(|| {
        entry
            .as_ref()
            .map(|entry| entry.name.clone())
            .unwrap_or_default()
    });
    let mut directory = use_signal(|| {
        entry
            .as_ref()
            .map(|entry| entry.path.clone())
            .unwrap_or_default()
    });
    let mut repository = use_signal(|| {
        entry
            .as_ref()
            .and_then(|entry| entry.repository.clone())
            .unwrap_or_default()
    });
    let mut source = use_signal(|| match &entry {
        Some(entry) if entry.repository.is_some() => Source::Github,
        Some(_) => Source::Local,
        None => Source::New,
    });
    let mut submitted = use_signal(|| false);
    let mut name_edited = use_signal(|| entry.is_some());
    let feedback = use_ui_state::<ProjectFormUiState>().state;
    let selected = feedback.read().clone();
    let matching = selected.request_id == request_id();
    let pending = submitted() || (matching && selected.pending);
    let error = if matching {
        selected.error.clone()
    } else {
        None
    };
    let project = entry.as_ref().map(|entry| entry.id.clone());
    let revision = entry.as_ref().map_or(0, |entry| entry.revision);
    let editing = entry.is_some();
    use_effect(move || {
        let feedback = feedback.read();
        if feedback.request_id != request_id() {
            return;
        }
        if let Some(folder) = &feedback.folder {
            directory.set(folder.clone());
            if !*name_edited.peek() {
                let label = std::path::Path::new(folder)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                name.set(label);
            }
        }
        if !feedback.pending {
            submitted.set(false);
        }
        if feedback.saved.is_some() {
            onclose.call(());
        }
    });
    let ready = !name.read().trim().is_empty()
        && match source() {
            Source::New => true,
            Source::Local => !directory.read().is_empty(),
            Source::Github => !repository.read().trim().is_empty(),
        };
    rsx! {
                form {
                    class: "flex flex-col gap-4",
                    onkeydown: move |event: KeyboardEvent| event.stop_propagation(),
                    onsubmit: move |event: FormEvent| {
                        event.prevent_default();
                        if !ready || pending || submitted() { return; }
                        let selected = match source() {
                            Source::New => ProjectSource::New,
                            Source::Local => ProjectSource::Local(directory()),
                            Source::Github => ProjectSource::Github(repository()),
                        };
                        let result = send(&ProjectSaveRequest { request_id: request_id(), project: project.clone(), revision, name: name(), source: selected });
                        if result.is_ok() { submitted.set(true); }
                    },
                    label { class: "flex flex-col gap-1.5 text-xs text-muted-foreground",
                        {translate("project-name-placeholder")}
                        input {
                            class: "rounded-md bg-foreground/[0.045] px-3 py-2 text-sm text-foreground outline-none focus:bg-foreground/[0.065]",
                            autofocus: true,
                            value: name(),
                            disabled: pending,
                            oninput: move |event: FormEvent| { name_edited.set(true); name.set(event.value()); },
                        }
                    }
                    div { class: "flex flex-col gap-2",
                        span { class: "text-xs text-muted-foreground", {translate("project-source")} }
                        div { class: "flex flex-wrap gap-2",
                            if !editing {
                                SourceChoice { value: Source::New, current: source(), disabled: pending, onchange: move |value| source.set(value) }
                            }
                            SourceChoice { value: Source::Local, current: source(), disabled: pending, onchange: move |value| source.set(value) }
                            SourceChoice { value: Source::Github, current: source(), disabled: pending, onchange: move |value| source.set(value) }
                        }
                    }
                    if source() == Source::Local {
                        div { class: "flex min-w-0 items-center gap-2",
                            span { class: "min-w-0 flex-1 truncate rounded-md bg-foreground/[0.045] px-3 py-2 text-xs text-muted-foreground", title: directory(), "{directory}" }
                            Button {
                                size: ButtonSize::Sm,
                                variant: ButtonVariant::Ghost,
                                disabled: pending,
                                onclick: move |_| {
                                    if submitted() { return; }
                                    if send(&ProjectFolderPickRequest { request_id: request_id(), directory: directory() }).is_ok() { submitted.set(true); }
                                },
                                {translate("agent-project-choose")}
                            }
                        }
                    } else if source() == Source::Github {
                        label { class: "flex flex-col gap-1.5 text-xs text-muted-foreground",
                            {translate("project-github-repository")}
                            input {
                                class: "rounded-md bg-foreground/[0.045] px-3 py-2 text-sm text-foreground outline-none focus:bg-foreground/[0.065]",
                                placeholder: translate("project-repository-placeholder"),
                                value: repository(),
                                disabled: pending,
                                oninput: move |event: FormEvent| {
                                    let value = event.value();
                                    if !name_edited() && let Some(label) = value.trim().trim_end_matches('/').rsplit('/').next() { name.set(label.trim_end_matches(".git").to_string()); }
                                    repository.set(value);
                                },
                            }
                            span { class: "text-[11px]", {translate("project-clone-description")} }
                        }
                    }
                    if let Some(error) = error {
                        p { role: "alert", class: "text-xs text-destructive", "{error}" }
                    }
                    div { class: "flex justify-end gap-2",
                        Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, onclick: move |_| onclose.call(()), {translate(if pending { "common-close" } else { "common-cancel" })} }
                        button {
                            r#type: "submit",
                            class: "rounded-md bg-primary px-4 py-2 text-xs font-medium text-primary-foreground disabled:opacity-50",
                            disabled: !ready || pending,
                            {translate(if editing { "common-save" } else { "project-create" })}
                        }
                    }
                }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
    New,
    Local,
    Github,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Self::New => "project-new-repository",
            Self::Local => "project-local-folder",
            Self::Github => "project-github-repository",
        }
    }

    fn icon(self) -> LineIcon {
        match self {
            Self::New => LineIcon::Plus,
            Self::Local => LineIcon::FileOutput,
            Self::Github => LineIcon::GitFork,
        }
    }
}

#[component]
fn SourceChoice(
    value: Source,
    current: Source,
    disabled: bool,
    onchange: EventHandler<Source>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if value == current { "flex items-center gap-1.5 rounded-md bg-foreground/10 px-3 py-2 text-xs text-foreground" } else { "flex items-center gap-1.5 rounded-md bg-foreground/[0.025] px-3 py-2 text-xs text-muted-foreground hover:bg-foreground/5" },
            aria_pressed: value == current,
            aria_label: translate(value.label()),
            disabled,
            onclick: move |_| onchange.call(value),
            LineIconView { icon: value.icon(), class: "size-3.5" }
            {translate(value.label())}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation};
    use dioxus_html::SerializedHtmlEventConverter;
    use dioxus_html::{EventData, SerializedMouseData};

    #[component]
    fn FormPreview() -> Element {
        rsx! { ProjectForm { entry: None, onclose: move |_| {} } }
    }

    #[test]
    fn github_and_local_choices_switch_the_creation_form_inputs() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(FormPreview);
        let rendered = dom.rebuild_to_vec();
        let mut github = None;
        let mut local = None;
        for edit in &rendered.edits {
            if let Mutation::SetAttribute {
                name: "aria-label",
                value: AttributeValue::Text(value),
                id,
                ..
            } = edit
            {
                if value == &translate("project-github-repository") {
                    github = Some(*id);
                }
                if value == &translate("project-local-folder") {
                    local = Some(*id);
                }
            }
        }
        assert!(!dioxus_ssr::render(&dom).contains(&translate("project-repository-placeholder")));
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            github.expect("GitHub source choice"),
        );
        dom.render_immediate_to_vec();
        assert!(dioxus_ssr::render(&dom).contains(&translate("project-repository-placeholder")));
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            local.expect("local source choice"),
        );
        dom.render_immediate_to_vec();
        let local = dioxus_ssr::render(&dom);
        assert!(!local.contains(&translate("project-repository-placeholder")));
        assert!(local.contains(&translate("agent-project-choose")));
    }
}
