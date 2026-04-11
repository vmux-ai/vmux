#![allow(clippy::type_complexity)]

mod chrome_state;
mod common;
mod cursor_icon;
mod keyboard;
mod loading_state;
mod mute;
mod navigation;
mod system_param;
mod webview;
mod zoom;

use crate::common::{LocalHostPlugin, MessageLoopPlugin, WebviewCoreComponentsPlugin};
use crate::cursor_icon::SystemCursorIconPlugin;
use crate::keyboard::KeyboardPlugin;
use crate::chrome_state::WebviewChromeStatePlugin;
use crate::loading_state::WebviewLoadingStatePlugin;
use crate::mute::AudioMutePlugin;
use crate::prelude::{IpcPlugin, NavigationPlugin, WebviewPlugin};
use crate::zoom::ZoomPlugin;
use bevy::prelude::*;
use bevy_cef_core::prelude::{
    CefEmbeddedHosts, CefEmbeddedPageConfig, CefExtensions, CommandLineConfig,
    compile_time_cef_embedded_scheme, try_set_cef_embedded_page_config,
};
use bevy_remote::RemotePlugin;

pub mod prelude {
    pub use crate::{
        CefPlugin, RunOnMainThread, chrome_state::*, common::*, keyboard::CefKeyboardInputSet,
        loading_state::*, navigation::*, webview::prelude::*,
    };
    pub use bevy_cef_core::prelude::{
        Browsers, CefDiskProfileRoot, CefEmbeddedHost, CefEmbeddedHosts, CefEmbeddedPageConfig,
        CefExtensions, CommandLineConfig, WebviewChromeStateEvent, WebviewLoadingStateEvent,
        compile_time_cef_embedded_scheme, resolved_cef_embedded_page_config,
    };
}

pub struct RunOnMainThread;

#[derive(Debug, Clone)]
pub struct CefPlugin {
    pub command_line_config: CommandLineConfig,
    pub extensions: CefExtensions,
    pub root_cache_path: Option<String>,
    pub embedded_scheme: String,
    pub embedded_hosts: CefEmbeddedHosts,
}

impl Default for CefPlugin {
    fn default() -> Self {
        Self {
            command_line_config: CommandLineConfig::default(),
            extensions: CefExtensions::default(),
            root_cache_path: None,
            embedded_scheme: compile_time_cef_embedded_scheme().to_string(),
            embedded_hosts: CefEmbeddedHosts::default(),
        }
    }
}

impl Plugin for CefPlugin {
    fn build(&self, app: &mut App) {
        try_set_cef_embedded_page_config(CefEmbeddedPageConfig::new(
            self.embedded_scheme.clone(),
            self.embedded_hosts.clone(),
        ));
        app.insert_resource(bevy_cef_core::prelude::CefDiskProfileRoot(
            self.root_cache_path.clone(),
        ));
        app.add_plugins((
            LocalHostPlugin,
            MessageLoopPlugin {
                config: self.command_line_config.clone(),
                extensions: self.extensions.clone(),
                root_cache_path: self.root_cache_path.clone(),
            },
            WebviewCoreComponentsPlugin,
            WebviewPlugin,
            IpcPlugin,
            KeyboardPlugin,
            SystemCursorIconPlugin,
            WebviewLoadingStatePlugin,
            WebviewChromeStatePlugin,
            NavigationPlugin,
            ZoomPlugin,
            AudioMutePlugin,
        ));
        if !app.is_plugin_added::<RemotePlugin>() {
            app.add_plugins(RemotePlugin::default());
        }
    }
}
