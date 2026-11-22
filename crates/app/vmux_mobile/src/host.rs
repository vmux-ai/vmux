use std::rc::Rc;

use vmux_api::BinEvent;
use vmux_api::command_bar::{
    CommandBarUiState, DismissRequest as CommandBarDismissRequest,
    ExRequest as CommandBarExRequest, InvokeRequest as CommandBarInvokeRequest,
    OpenRequest as CommandBarOpenRequest, PickRequest as CommandBarPickRequest,
    PromptRequest as CommandBarPromptRequest, SwitchTabRequest,
};
use vmux_api::prompt_media::ChatAttachPaths;
use vmux_api::team::TeamUiState;
use vmux_session::event::{
    ChatApproval, ChatCancel, ChatDraftChanged, ChatEscape, ChatRemoveAttachment, ChatStop,
    ChatSubmit, SelectModel, SetAgentEffort,
};
use vmux_session::host::RepublishChatUiState;
use vmux_session::state::ChatUiState;
use vmux_start::roster::RepublishLauncher;
use vmux_team::roster::RepublishTeam;
use vmux_ui::hooks::EventListenerError;
use vmux_ui::hooks::transport::{BytesListener, HostBinding, HostPayload, PageHost};

use crate::actions::{
    ApproveCallIntent, AttachPathsIntent, CancelChatIntent, ChangeDraftIntent, EscapeChatIntent,
    MountInterestAdded, MountInterestKind, RemoveAttachmentIntent, SelectModelIntent,
    SetEffortIntent, SubmitPromptIntent,
};
use crate::presentation::{
    MobileConnectionUiState, MobileSessionUiState, RepublishMobileConnection,
    RepublishMobileSession,
};
use crate::runtime::{MountId, RuntimeHandle};
use crate::session::{StartChatRequest, SwitchTabIntent};

pub(crate) struct MobileHost {
    runtime: RuntimeHandle,
    mount: MountId,
}

impl MobileHost {
    pub(crate) fn mount(runtime: RuntimeHandle) -> (HostBinding, MountLease) {
        let mount = runtime.allocate_mount();
        let host = Rc::new(Self {
            runtime: runtime.clone(),
            mount,
        });
        (HostBinding(host), MountLease { runtime, mount })
    }

    fn decode<T>(bytes: &[u8]) -> Result<T, EventListenerError>
    where
        T: rkyv::Archive,
        T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
            + for<'a> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
            >,
    {
        HostPayload::new(bytes)
            .decode::<T>()
            .ok_or(EventListenerError::SerializePayload)
    }
}

pub(crate) struct MountLease {
    runtime: RuntimeHandle,
    mount: MountId,
}

impl Drop for MountLease {
    fn drop(&mut self) {
        self.runtime.unmount(self.mount);
    }
}

impl PageHost for MobileHost {
    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        match id {
            ChatSubmit::ID => {
                let payload: ChatSubmit = Self::decode(bytes)?;
                self.runtime.send(SubmitPromptIntent { text: payload.text });
                Ok(())
            }
            ChatDraftChanged::ID => {
                let payload: ChatDraftChanged = Self::decode(bytes)?;
                self.runtime.send(ChangeDraftIntent { text: payload.text });
                Ok(())
            }
            ChatRemoveAttachment::ID => {
                let payload: ChatRemoveAttachment = Self::decode(bytes)?;
                self.runtime
                    .send(RemoveAttachmentIntent { path: payload.path });
                Ok(())
            }
            ChatCancel::ID | ChatStop::ID => {
                self.runtime.send(CancelChatIntent);
                Ok(())
            }
            ChatEscape::ID => {
                self.runtime.send(EscapeChatIntent);
                Ok(())
            }
            ChatApproval::ID => {
                let payload: ChatApproval = Self::decode(bytes)?;
                self.runtime.send(ApproveCallIntent {
                    call_id: payload.call_id,
                    decision: payload.decision,
                });
                Ok(())
            }
            SelectModel::ID => {
                let payload: SelectModel = Self::decode(bytes)?;
                self.runtime.send(SelectModelIntent {
                    model_id: payload.model_id,
                });
                Ok(())
            }
            SetAgentEffort::ID => {
                let payload: SetAgentEffort = Self::decode(bytes)?;
                self.runtime.send(SetEffortIntent {
                    level: payload.level,
                });
                Ok(())
            }
            ChatAttachPaths::ID => {
                let payload: ChatAttachPaths = Self::decode(bytes)?;
                self.runtime.send(AttachPathsIntent {
                    paths: payload.paths,
                });
                Ok(())
            }
            CommandBarPromptRequest::ID => {
                let request: CommandBarPromptRequest = Self::decode(bytes)?;
                self.runtime.send(StartChatRequest {
                    text: request.text,
                    agent_url: request.target_url,
                });
                Ok(())
            }
            SwitchTabRequest::ID => {
                let request: SwitchTabRequest = Self::decode(bytes)?;
                self.runtime.send(SwitchTabIntent {
                    index: request.index,
                });
                Ok(())
            }
            CommandBarDismissRequest::ID => Ok(()),
            CommandBarOpenRequest::ID
            | CommandBarInvokeRequest::ID
            | CommandBarExRequest::ID
            | CommandBarPickRequest::ID => Err(EventListenerError::Unsupported),
            _ => Err(EventListenerError::Unsupported),
        }
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        match id {
            MobileConnectionUiState::ID => {
                self.runtime.listen(
                    self.mount,
                    MobileConnectionUiState::ID,
                    on_bytes,
                    RepublishMobileConnection,
                );
            }
            MobileSessionUiState::ID => {
                self.runtime.listen(
                    self.mount,
                    MobileSessionUiState::ID,
                    on_bytes,
                    RepublishMobileSession,
                );
            }
            ChatUiState::ID => {
                self.runtime.send(MountInterestAdded {
                    mount: self.mount,
                    kind: MountInterestKind::Chat,
                });
                self.runtime
                    .listen(self.mount, ChatUiState::ID, on_bytes, RepublishChatUiState);
            }
            CommandBarUiState::ID => {
                self.runtime.listen(
                    self.mount,
                    CommandBarUiState::ID,
                    on_bytes,
                    RepublishLauncher,
                );
            }
            TeamUiState::ID => {
                self.runtime.send(MountInterestAdded {
                    mount: self.mount,
                    kind: MountInterestKind::Team,
                });
                self.runtime
                    .listen(self.mount, TeamUiState::ID, on_bytes, RepublishTeam);
            }
            _ => return Err(EventListenerError::Unsupported),
        }
        Ok(())
    }
}
