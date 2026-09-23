#[cfg(feature = "core")]
pub use vmux_command::{ClaimedUrl, ContributedCommand, ContributedPage};
#[cfg(feature = "core")]
pub use vmux_core::{ContributedCommandChosen, page::PageManifest};
#[cfg(feature = "layout")]
pub use vmux_layout::native_open::{HostedPage, HostedPagePlugin};
#[cfg(feature = "mcp")]
pub use vmux_mcp::{
    protocol::{McpServer, McpServerBuilder},
    tool::{DispatchTarget, McpToolPlugin, McpToolRequest, ToolDispatchSet},
};
