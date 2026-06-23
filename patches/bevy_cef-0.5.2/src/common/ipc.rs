mod bin_host_emit;
mod bin_js_emit;
mod dom_snapshot;
mod host_emit;
mod js_emit;

use crate::common::ipc::bin_js_emit::BinIpcRawEventPlugin;
use crate::common::ipc::js_emit::IpcRawEventPlugin;
use bevy::prelude::*;

use crate::common::ipc::bin_host_emit::BinHostEmitPlugin;
use crate::common::ipc::dom_snapshot::DomSnapshotPlugin;
use crate::common::ipc::host_emit::HostEmitPlugin;
pub use bin_host_emit::*;
pub use bin_js_emit::*;
pub use dom_snapshot::*;
pub use host_emit::*;
pub use js_emit::*;

pub struct IpcPlugin;

impl Plugin for IpcPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IpcRawEventPlugin,
            HostEmitPlugin,
            BinHostEmitPlugin,
            BinIpcRawEventPlugin,
            DomSnapshotPlugin,
        ));
    }
}
