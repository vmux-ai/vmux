use super::PreviewAccess;
use crate::edit::core::EditCoreItem;
use crate::edit::ex::ExRange;
use crate::edit::{EditCommand, EditCore, EditMode, EditOutcome, Motion, Operator, Target};
use bevy::prelude::*;

pub(crate) struct DocumentPlugin;

impl Plugin for DocumentPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(apply::<CompletionInsertRequest>);
        app.add_observer(apply::<CommittedTextRequest>);
        app.add_observer(apply::<PasteTextRequest>);
        app.add_observer(apply::<MoveRequest>);
        app.add_observer(apply::<SelectRequest>);
        app.add_observer(apply::<InsertTextRequest>);
        app.add_observer(apply::<OvertypeTextRequest>);
        app.add_observer(apply::<ReplaceTextRequest>);
        app.add_observer(apply::<InsertNewlineRequest>);
        app.add_observer(apply::<InsertTabRequest>);
        app.add_observer(apply::<DeleteBackRequest>);
        app.add_observer(apply::<DeleteForwardRequest>);
        app.add_observer(apply::<DeleteWordBackRequest>);
        app.add_observer(apply::<OperatorRequest>);
        app.add_observer(apply::<PutRequest>);
        app.add_observer(apply::<ReplaceCharRequest>);
        app.add_observer(apply::<JoinLinesRequest>);
        app.add_observer(apply::<OpenLineRequest>);
        app.add_observer(apply::<ModeRequest>);
        app.add_observer(apply::<UndoRequest>);
        app.add_observer(apply::<RedoRequest>);
        app.add_observer(apply::<UndoTimeRequest>);
        app.add_observer(apply::<SearchRequest>);
        app.add_observer(apply::<SearchWordRequest>);
        app.add_observer(apply::<ClearSearchHighlightRequest>);
        app.add_observer(apply::<SubstituteRequest>);
        app.add_observer(apply::<DeleteLinesRequest>);
        app.add_observer(apply::<YankLinesRequest>);
        app.add_observer(apply::<ReshapeRequest>);
        app.add_observer(apply::<MarkRequest>);
        app.add_observer(apply::<GotoMarkRequest>);
        app.add_observer(apply::<JumpRequest>);
        app.add_observer(apply::<ChangeRequest>);
        app.add_observer(apply::<BeginBlockInsertRequest>);
        app.add_observer(apply::<FinishBlockInsertRequest>);
        app.add_observer(apply::<IncrementRequest>);
        app.add_observer(apply::<SwapSelectionRequest>);
        app.add_observer(apply::<SelectObjectRequest>);
        app.add_observer(apply::<SelectOccurrencesRequest>);
        app.add_observer(apply::<CollapseCaretsRequest>);
        app.add_observer(apply::<AddCaretRequest>);
    }
}

#[derive(EntityEvent)]
pub(super) struct Edited {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: uuid::Uuid,
    pub(super) outcome: EditOutcome,
    pub(super) cursor_changed: bool,
}

trait Intent: EntityEvent {
    const PER_CARET: bool;
    const PREVIEW_ALLOWED: bool;
    const CLIPBOARD_REGISTER: bool = false;
    const RENDER_CURSOR: bool = false;
    const CLOSE_FIND: bool = false;
    fn preserves_vertical(&self) -> bool {
        false
    }
    fn accepted_text(&self) -> Option<&str> {
        None
    }
    fn document(&self) -> Option<uuid::Uuid>;
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>);
}

fn apply<T: Intent>(
    trigger: On<T>,
    mut documents: Query<(
        EditCore,
        Option<&mut crate::keymap::vim::VimKeymap>,
        Option<&super::FileViewport>,
    )>,
    preview: PreviewAccess,
    mut clipboard: Option<NonSendMut<super::ClipboardHandle>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let request = trigger.event();
    let Ok((mut core, mut parser, viewport)) = documents.get_mut(entity) else {
        return;
    };
    if request
        .document()
        .is_some_and(|document| core.revision.identity != document)
    {
        return;
    }
    if !T::PREVIEW_ALLOWED && preview.read_only(entity, &core.buffer.path) {
        return;
    }
    if let Some(viewport) = viewport {
        core.viewport.sync(viewport);
    }
    if T::CLIPBOARD_REGISTER
        && let Some(clipboard) = clipboard.as_mut()
        && let Some(cb) = clipboard.0.as_mut()
        && let Ok(text) = cb.get_text()
        && text != core.registers.clipboard_shadow
    {
        core.registers.clipboard_shadow.clone_from(&text);
        core.registers
            .set_unnamed(crate::edit::RegisterValue::charwise(text));
    }
    if let Some(text) = request.accepted_text()
        && (text.is_empty() || !core.mode.value.accepts_text())
    {
        return;
    }
    let before = core.carets.selections.clone();
    let before_mode = core.mode.value;
    let before_dirty = core.revision.dirty;
    let was_active = core.carets.active;
    let mut order = vec![was_active];
    if T::PER_CARET {
        order = (0..core.carets.selections.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(core.carets.selections[i].range().start));
    }
    let mut outcome = EditOutcome::default();
    core.transaction.in_sweep = T::PER_CARET && order.len() > 1;
    core.transaction.sweep_snapshotted = false;
    for index in order {
        core.carets.active = index;
        if !request.preserves_vertical() {
            core.forget_vertical();
        }
        let (changed, yank) = request.applied(&mut core);
        outcome.text_changed |= changed;
        if yank.is_some() {
            outcome.yank = yank;
        }
    }
    if outcome.text_changed
        && let Some(text) = request.accepted_text()
        && let Some(parser) = parser.as_mut()
    {
        parser.record(before_mode, text);
        commands.trigger(super::WikiCompletionRequest::from(entity));
    }
    core.transaction.in_sweep = false;
    core.transaction.sweep_snapshotted = false;
    core.carets.active = was_active.min(core.carets.selections.len().saturating_sub(1));
    core.merge_overlapping_carets();
    outcome.sel_changed = core.carets.selections != before;
    outcome.mode_changed = core.mode.value != before_mode;
    outcome.dirty_changed = core.revision.dirty != before_dirty;
    if T::CLOSE_FIND {
        commands.trigger(super::FileFindCloseRequest::from(entity));
    }
    let cursor_changed = outcome.sel_changed || outcome.mode_changed || T::RENDER_CURSOR;
    commands.trigger(Edited {
        entity,
        cursor_changed,
        document: core.revision.identity,
        outcome,
    });
}

#[derive(EntityEvent, Clone)]
pub(super) struct MoveRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) motion: Motion,
}

impl Intent for MoveRequest {
    fn preserves_vertical(&self) -> bool {
        matches!(
            self.motion,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        )
    }
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.move_caret(self.motion)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct SelectRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) motion: Motion,
}

impl Intent for SelectRequest {
    fn preserves_vertical(&self) -> bool {
        matches!(
            self.motion,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        )
    }
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.select_caret(self.motion)
    }
}

#[derive(EntityEvent, Clone)]
pub(crate) struct InsertTextRequest {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) document: Option<uuid::Uuid>,
    pub(crate) text: String,
}

impl Intent for InsertTextRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.insert(self.text.clone())
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct OvertypeTextRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) text: String,
}

impl Intent for OvertypeTextRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.overtype(self.text.clone())
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct ReplaceTextRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) text: String,
}

impl Intent for ReplaceTextRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.replace(self.text.clone())
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct InsertNewlineRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for InsertNewlineRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.newline()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct InsertTabRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for InsertTabRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.tab()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct DeleteBackRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for DeleteBackRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.backspace()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct DeleteForwardRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for DeleteForwardRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.delete_forward()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct DeleteWordBackRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for DeleteWordBackRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.delete_word_back()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct OperatorRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) operator: Operator,
    pub(super) target: Target,
    pub(super) register: Option<char>,
}

impl Intent for OperatorRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.operate(self.operator, self.target, self.register)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct PutRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) before: bool,
    pub(super) count: usize,
    pub(super) register: Option<char>,
}

impl Intent for PutRequest {
    const CLIPBOARD_REGISTER: bool = true;
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.put(self.before, self.count, self.register)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct ReplaceCharRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) ch: char,
    pub(super) count: usize,
}

impl Intent for ReplaceCharRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.replace_char(self.ch, self.count)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct JoinLinesRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) count: usize,
    pub(super) spaces: bool,
}

impl Intent for JoinLinesRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.join_lines(self.count, self.spaces)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct OpenLineRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) above: bool,
}

impl Intent for OpenLineRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.open_line(self.above)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct ModeRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) mode: EditMode,
}

impl Intent for ModeRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.set_mode(self.mode)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct UndoRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for UndoRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.undo()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct RedoRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for RedoRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.redo()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct UndoTimeRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) forward: bool,
    pub(super) count: usize,
}

impl Intent for UndoTimeRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.undo_time(self.forward, self.count)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct SearchRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) pattern: String,
    pub(super) forward: bool,
}

impl Intent for SearchRequest {
    const RENDER_CURSOR: bool = true;
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.set_search(self.pattern.clone(), self.forward)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct SearchWordRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) forward: bool,
}

impl Intent for SearchWordRequest {
    const RENDER_CURSOR: bool = true;
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.search_word(self.forward)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct ClearSearchHighlightRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for ClearSearchHighlightRequest {
    const CLOSE_FIND: bool = true;
    const RENDER_CURSOR: bool = true;
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.clear_search()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct SubstituteRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) range: ExRange,
    pub(super) pattern: String,
    pub(super) replacement: String,
    pub(super) all: bool,
}

impl Intent for SubstituteRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.substitution(
            self.range,
            self.pattern.clone(),
            self.replacement.clone(),
            self.all,
        )
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct DeleteLinesRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) range: ExRange,
}

impl Intent for DeleteLinesRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.delete_lines(self.range)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct YankLinesRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) range: ExRange,
}

impl Intent for YankLinesRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.yank_lines(self.range)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct ReshapeRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) shape: crate::shape::BufferShape,
}

impl Intent for ReshapeRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.reshape_document(self.shape)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct MarkRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) name: char,
}

impl Intent for MarkRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.set_mark(self.name)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct GotoMarkRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) name: char,
    pub(super) linewise: bool,
}

impl Intent for GotoMarkRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.goto_mark(self.name, self.linewise)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct JumpRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) back: bool,
    pub(super) count: usize,
}

impl Intent for JumpRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.jump_list(self.back, self.count)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct ChangeRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) back: bool,
    pub(super) count: usize,
}

impl Intent for ChangeRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.change_list(self.back, self.count)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct BeginBlockInsertRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) after: bool,
}

impl Intent for BeginBlockInsertRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.begin_block(self.after)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct FinishBlockInsertRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) text: String,
}

impl Intent for FinishBlockInsertRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.finish_block(self.text.clone())
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct IncrementRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) delta: i64,
}

impl Intent for IncrementRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.increment(self.delta)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct SwapSelectionRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for SwapSelectionRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.swap_selection()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct SelectObjectRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) object: crate::edit::TextObject,
}

impl Intent for SelectObjectRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.select_object(self.object)
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct SelectOccurrencesRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for SelectOccurrencesRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.select_occurrences()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct CollapseCaretsRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
}

impl Intent for CollapseCaretsRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.collapse()
    }
}

#[derive(EntityEvent, Clone)]
pub(super) struct AddCaretRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) direction: crate::edit::command::VerticalDirection,
}

impl Intent for AddCaretRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = true;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        core.add_caret(self.direction)
    }
}

#[derive(EntityEvent)]
pub(super) struct PasteTextRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) text: String,
}

impl Intent for PasteTextRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        (core.paste(&self.text), None)
    }
}

#[derive(EntityEvent)]
pub(super) struct CommittedTextRequest {
    #[event_target]
    pub(super) entity: Entity,
    pub(super) document: Option<uuid::Uuid>,
    pub(super) text: String,
}

impl Intent for CommittedTextRequest {
    const PER_CARET: bool = true;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn accepted_text(&self) -> Option<&str> {
        Some(&self.text)
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        if core.mode.value == EditMode::Replace {
            core.overtype(self.text.clone())
        } else {
            core.insert(self.text.clone())
        }
    }
}

#[derive(EntityEvent)]
pub(crate) struct CompletionInsertRequest {
    #[event_target]
    pub(crate) entity: Entity,
    pub(crate) document: Option<uuid::Uuid>,
    pub(crate) text: String,
    pub(crate) line: u32,
    pub(crate) replace_from_col: u32,
}

impl Intent for CompletionInsertRequest {
    const PER_CARET: bool = false;
    const PREVIEW_ALLOWED: bool = false;
    fn document(&self) -> Option<uuid::Uuid> {
        self.document
    }
    fn applied(
        &self,
        core: &mut EditCoreItem<'_, '_>,
    ) -> (bool, Option<crate::edit::RegisterValue>) {
        let start = core
            .buffer
            .coords_to_char(self.line as usize, self.replace_from_col as usize);
        let head = core.primary().head;
        core.carets.selections = vec![crate::edit::Selection {
            anchor: start.min(head),
            head: start.max(head),
        }];
        core.insert(self.text.clone())
    }
}

impl EditCommand {
    pub(super) fn emit_document(
        self,
        entity: Entity,
        document: Option<uuid::Uuid>,
        commands: &mut Commands,
    ) -> Option<Self> {
        let command = self.encode_text(entity, document, commands)?;
        let command = command.encode_selection(entity, document, commands)?;
        let command = command.encode_history(entity, document, commands)?;
        command.encode_operator(entity, document, commands)
    }

    fn encode_text(
        self,
        entity: Entity,
        document: Option<uuid::Uuid>,
        commands: &mut Commands,
    ) -> Option<Self> {
        match self {
            EditCommand::InsertText(t) => {
                commands.trigger(InsertTextRequest {
                    entity,
                    document,
                    text: t,
                });
            }
            EditCommand::OvertypeText(t) => {
                commands.trigger(OvertypeTextRequest {
                    entity,
                    document,
                    text: t,
                });
            }
            EditCommand::ReplaceText(t) => {
                commands.trigger(ReplaceTextRequest {
                    entity,
                    document,
                    text: t,
                });
            }
            EditCommand::InsertNewline => {
                commands.trigger(InsertNewlineRequest { entity, document });
            }
            EditCommand::InsertTab => {
                commands.trigger(InsertTabRequest { entity, document });
            }
            EditCommand::DeleteBack => {
                commands.trigger(DeleteBackRequest { entity, document });
            }
            EditCommand::DeleteForward => {
                commands.trigger(DeleteForwardRequest { entity, document });
            }
            EditCommand::DeleteWordBack => {
                commands.trigger(DeleteWordBackRequest { entity, document });
            }
            EditCommand::ReplaceChar { ch, count } => {
                commands.trigger(ReplaceCharRequest {
                    entity,
                    document,
                    ch,
                    count,
                });
            }
            effect => return Some(effect),
        }
        None
    }

    fn encode_selection(
        self,
        entity: Entity,
        document: Option<uuid::Uuid>,
        commands: &mut Commands,
    ) -> Option<Self> {
        match self {
            EditCommand::Move(m) => {
                commands.trigger(MoveRequest {
                    entity,
                    document,
                    motion: m,
                });
            }
            EditCommand::Select(m) => {
                commands.trigger(SelectRequest {
                    entity,
                    document,
                    motion: m,
                });
            }
            EditCommand::SetMode(m) => {
                commands.trigger(ModeRequest {
                    entity,
                    document,
                    mode: m,
                });
            }
            EditCommand::BeginBlockInsert { after } => {
                commands.trigger(BeginBlockInsertRequest {
                    entity,
                    document,
                    after,
                });
            }
            EditCommand::FinishBlockInsert { text } => {
                commands.trigger(FinishBlockInsertRequest {
                    entity,
                    document,
                    text,
                });
            }
            EditCommand::SwapSelectionEnds => {
                commands.trigger(SwapSelectionRequest { entity, document });
            }
            EditCommand::SelectTextObject(obj) => {
                commands.trigger(SelectObjectRequest {
                    entity,
                    document,
                    object: obj,
                });
            }
            EditCommand::SelectAllOccurrences => {
                commands.trigger(SelectOccurrencesRequest { entity, document });
            }
            EditCommand::CollapseCarets => {
                commands.trigger(CollapseCaretsRequest { entity, document });
            }
            EditCommand::AddCaretVertically(direction) => {
                commands.trigger(AddCaretRequest {
                    entity,
                    document,
                    direction,
                });
            }
            effect => return Some(effect),
        }
        None
    }

    fn encode_history(
        self,
        entity: Entity,
        document: Option<uuid::Uuid>,
        commands: &mut Commands,
    ) -> Option<Self> {
        match self {
            EditCommand::Undo => {
                commands.trigger(UndoRequest { entity, document });
            }
            EditCommand::Redo => {
                commands.trigger(RedoRequest { entity, document });
            }
            EditCommand::UndoTime { forward, count } => {
                commands.trigger(UndoTimeRequest {
                    entity,
                    document,
                    forward,
                    count,
                });
            }
            EditCommand::SetSearch { pattern, forward } => {
                commands.trigger(SearchRequest {
                    entity,
                    document,
                    pattern,
                    forward,
                });
            }
            EditCommand::SearchWord { forward } => {
                commands.trigger(SearchWordRequest {
                    entity,
                    document,
                    forward,
                });
            }
            EditCommand::ClearSearchHighlight => {
                commands.trigger(ClearSearchHighlightRequest { entity, document });
            }
            EditCommand::SetMark(name) => {
                commands.trigger(MarkRequest {
                    entity,
                    document,
                    name,
                });
            }
            EditCommand::GotoMark { name, linewise } => {
                commands.trigger(GotoMarkRequest {
                    entity,
                    document,
                    name,
                    linewise,
                });
            }
            EditCommand::JumpList { back, count } => {
                commands.trigger(JumpRequest {
                    entity,
                    document,
                    back,
                    count,
                });
            }
            EditCommand::ChangeList { back, count } => {
                commands.trigger(ChangeRequest {
                    entity,
                    document,
                    back,
                    count,
                });
            }
            effect => return Some(effect),
        }
        None
    }

    fn encode_operator(
        self,
        entity: Entity,
        document: Option<uuid::Uuid>,
        commands: &mut Commands,
    ) -> Option<Self> {
        match self {
            EditCommand::Op {
                operator,
                target,
                register,
            } => {
                commands.trigger(OperatorRequest {
                    entity,
                    document,
                    operator,
                    target,
                    register,
                });
            }
            EditCommand::Put {
                before,
                count,
                register,
            } => {
                commands.trigger(PutRequest {
                    entity,
                    document,
                    before,
                    count,
                    register,
                });
            }
            EditCommand::JoinLines { count, spaces } => {
                commands.trigger(JoinLinesRequest {
                    entity,
                    document,
                    count,
                    spaces,
                });
            }
            EditCommand::OpenLine { above } => {
                commands.trigger(OpenLineRequest {
                    entity,
                    document,
                    above,
                });
            }
            EditCommand::Substitute {
                range,
                pattern,
                replacement,
                all,
            } => {
                commands.trigger(SubstituteRequest {
                    entity,
                    document,
                    range,
                    pattern,
                    replacement,
                    all,
                });
            }
            EditCommand::ExDelete(range) => {
                commands.trigger(DeleteLinesRequest {
                    entity,
                    document,
                    range,
                });
            }
            EditCommand::ExYank(range) => {
                commands.trigger(YankLinesRequest {
                    entity,
                    document,
                    range,
                });
            }
            EditCommand::Reshape(shape) => {
                commands.trigger(ReshapeRequest {
                    entity,
                    document,
                    shape,
                });
            }
            EditCommand::Increment(delta) => {
                commands.trigger(IncrementRequest {
                    entity,
                    document,
                    delta,
                });
            }
            effect => return Some(effect),
        }
        None
    }
}
