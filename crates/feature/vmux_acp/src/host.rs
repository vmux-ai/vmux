pub use plugin::AcpPlugin;

mod plugin;

mod acp;
mod approval;
mod approval_driver;
mod attach;
mod attach_driver;
mod attention;
mod attention_driver;
mod command;
mod command_bar;
mod continuation;
mod event;
mod follow;
mod follow_driver;
mod handoff;
mod handoff_driver;
mod ingress;
mod model_selection;
mod navigation;
mod navigation_driver;
mod runtime;
mod runtime_driver;
mod toast;

#[cfg(test)]
mod test_support;

mod tidy;
mod tidy_driver;
