pub use event_request::{EventOutcome, EventRequest, EventRequestError};
pub use history_swipe::{HistoryDirection, HistorySwipe};
#[cfg(feature = "host")]
pub use host::{
    HostedPage, PageAppearance, PageBounds, PageFocusRequest, PageHostMode, PageHostPlugin,
    PageHostSet, PageLifecycle, PageRoot, PageScriptRequest, PageStylesheet,
};
#[cfg(feature = "hot-reload")]
pub use host_hot_reload::{HotReloadPlugin, HotReloadStylesheet, StylesheetReloaded};
pub use instance::{Instance, PageScope};
pub use page::Page;
pub use page_dom::{PageComponent, PageDom};
pub use registration::{PageClaimDirty, PagePlacement, PagePlugin, PageRegistration};
pub use shell::InterpreterShell;
pub use vmux_macro::page;
#[cfg(ui)]
pub use webview::{
    Appearance, AssetReply, Assets, Embedding, LifecycleSink, LifecycleUpdate, Outbox,
    SiblingOrder, Wake, WebView,
};

mod event_request;
mod history_swipe;
#[cfg(feature = "host")]
pub mod host;
#[cfg(feature = "hot-reload")]
mod host_hot_reload;
#[cfg(all(feature = "native", target_os = "macos"))]
mod host_native;
mod instance;
mod page;
mod page_dom;
mod page_dom_converter;
mod registration;
mod shell;

#[cfg(ui)]
mod webview;
