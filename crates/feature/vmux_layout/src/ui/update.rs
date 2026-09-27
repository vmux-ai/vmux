#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::progress::{Progress, ProgressIndicator};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[derive(Clone, PartialEq)]
pub(crate) enum UpdatePhase {
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Ready {
        version: String,
    },
}

impl From<&crate::event::UpdateProgress> for UpdatePhase {
    fn from(event: &crate::event::UpdateProgress) -> Self {
        if event.installing {
            return Self::Installing {
                version: event.version.clone(),
            };
        }
        Self::Downloading {
            version: event.version.clone(),
            downloaded: event.downloaded,
            total: event.total,
        }
    }
}

impl From<&crate::event::UpdateReady> for UpdatePhase {
    fn from(event: &crate::event::UpdateReady) -> Self {
        Self::Ready {
            version: event.version.clone(),
        }
    }
}

#[component]
pub(crate) fn UpdateNoticeFooter(phase: UpdatePhase) -> Element {
    let (label, version) = match &phase {
        UpdatePhase::Downloading { version, .. } => {
            (translate("layout-update-downloading"), version.clone())
        }
        UpdatePhase::Installing { version } => {
            (translate("layout-update-installing"), version.clone())
        }
        UpdatePhase::Ready { version } => (translate("layout-update-ready"), version.clone()),
    };
    rsx! {
        div {
            class: "shrink-0 mx-2 mb-2 mt-2 flex flex-col gap-2.5 rounded-md glass px-3 py-2.5 text-foreground",
            div { class: "flex min-w-0 items-center gap-2.5",
                span { class: "inline-block h-2 w-2 shrink-0 rounded-full bg-success" }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate text-ui font-medium leading-tight", "{label}" }
                    div { class: "mt-0.5 truncate text-xs leading-tight text-muted-foreground", "{version}" }
                }
            }
            {match phase {
                UpdatePhase::Downloading { downloaded, total, .. } => rsx! {
                    UpdateProgressBar { downloaded, total }
                },
                UpdatePhase::Installing { .. } => rsx! {
                    UpdateProgressBar { downloaded: 0, total: 0 }
                },
                UpdatePhase::Ready { .. } => rsx! {
                    button {
                        r#type: "button",
                        class: "w-full cursor-pointer rounded-md bg-primary px-2.5 py-1.5 text-ui font-medium text-primary-foreground hover:opacity-90",
                        onclick: move |_| {
                            let _ = send(&vmux_api::service::RelaunchRequest);
                        },
                        {translate("layout-restart-update")}
                    }
                },
            }}
        }
    }
}

#[component]
fn UpdateProgressBar(downloaded: u64, total: u64) -> Element {
    rsx! {
        Progress {
            value: (total > 0).then(|| download_pct(downloaded, total) as f64),
            attributes: vec![],
            ProgressIndicator { attributes: vec![] }
        }
    }
}

fn download_pct(downloaded: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    (downloaded.saturating_mul(100) / total).min(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_percentage_clamps_and_handles_zero_total() {
        assert_eq!(download_pct(10, 0), 0);
        assert_eq!(download_pct(25, 100), 25);
        assert_eq!(download_pct(125, 100), 100);
    }
}
