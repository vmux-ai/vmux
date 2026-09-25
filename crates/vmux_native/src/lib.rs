mod event_request;
mod instance;
mod page;
mod page_dom;
mod page_dom_converter;
mod registration;
mod shell;

pub use event_request::{EventOutcome, EventRequest, EventRequestError};
pub use instance::{Instance, PageScope};
pub use page::NativePage;
pub use page_dom::{PageComponent, PageDom};
pub use registration::{NativePagePlacement, NativePagePlugin, NativePageRegistration};
pub use shell::InterpreterShell;
pub use vmux_macro::page;

#[cfg(ui)]
mod webview;

#[cfg(ui)]
pub use webview::{Appearance, AssetReply, Assets, Embedding, Outbox, SiblingOrder, Wake, WebView};
