use super::*;
use crate::keymap::Mods;

fn key(k: &str, mods: Mods) -> KeyInput {
    KeyInput {
        key: k.into(),
        mods,
        repeat: false,
    }
}

#[test]
fn arrow_moves_shift_selects() {
    let mut km = VscodeKeymap;
    assert_eq!(
        km.handle(&key("ArrowRight", Mods::default())),
        vec![EditCommand::Move(Motion::Right)]
    );
    let shift = Mods {
        shift: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("ArrowRight", shift)),
        vec![EditCommand::Select(Motion::Right)]
    );
}

#[test]
fn cmd_chords() {
    let mut km = VscodeKeymap;
    let cmd = Mods {
        meta: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("c", cmd)),
        vec![selection_op(Operator::Yank)]
    );
    assert_eq!(km.handle(&key("s", cmd)), vec![EditCommand::Save]);
    let cmd_shift = Mods {
        meta: true,
        shift: true,
        ..Default::default()
    };
    assert_eq!(km.handle(&key("z", cmd_shift)), vec![EditCommand::Redo]);
}

#[test]
fn cmd_shift_bracket_folds() {
    let mut km = VscodeKeymap;
    let cs = Mods {
        meta: true,
        shift: true,
        ..Default::default()
    };
    assert_eq!(km.handle(&key("[", cs)), vec![EditCommand::FoldClose]);
    assert_eq!(km.handle(&key("]", cs)), vec![EditCommand::FoldOpen]);
    assert_eq!(km.handle(&key("J", cs)), vec![EditCommand::UnfoldAll]);
}

#[test]
fn select_all_composes() {
    let mut km = VscodeKeymap;
    let cmd = Mods {
        meta: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("a", cmd)),
        vec![
            EditCommand::Move(Motion::DocStart),
            EditCommand::Select(Motion::DocEnd)
        ]
    );
}

#[test]
fn word_backspace() {
    let mut km = VscodeKeymap;
    let alt = Mods {
        alt: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("Backspace", alt)),
        vec![EditCommand::DeleteWordBack]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn ctrl_emacs_nav_macos() {
    let mut km = VscodeKeymap;
    let ctrl = Mods {
        ctrl: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("a", ctrl)),
        vec![EditCommand::Move(Motion::LineStart)]
    );
    assert_eq!(
        km.handle(&key("e", ctrl)),
        vec![EditCommand::Move(Motion::LineEnd)]
    );
    assert_eq!(
        km.handle(&key("f", ctrl)),
        vec![EditCommand::Move(Motion::Right)]
    );
    assert_eq!(
        km.handle(&key("n", ctrl)),
        vec![EditCommand::Move(Motion::Down)]
    );
    assert_eq!(
        km.handle(&key("p", ctrl)),
        vec![EditCommand::Move(Motion::Up)]
    );
    assert_eq!(km.handle(&key("k", ctrl)), vec![delete_to_line_end()]);
    assert_eq!(km.handle(&key("h", ctrl)), vec![EditCommand::DeleteBack]);
}

#[cfg(target_os = "macos")]
#[test]
fn ctrl_shift_extends_macos() {
    let mut km = VscodeKeymap;
    let cs = Mods {
        ctrl: true,
        shift: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("A", cs)),
        vec![EditCommand::Select(Motion::LineStart)]
    );
    assert_eq!(
        km.handle(&key("E", cs)),
        vec![EditCommand::Select(Motion::LineEnd)]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn ctrl_is_not_gui_on_macos() {
    let mut km = VscodeKeymap;
    let ctrl = Mods {
        ctrl: true,
        ..Default::default()
    };
    assert_eq!(km.handle(&key("c", ctrl)), Vec::<EditCommand>::new());
    let meta = Mods {
        meta: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("c", meta)),
        vec![selection_op(Operator::Yank)]
    );
}

#[test]
fn lsp_actions() {
    let mut km = VscodeKeymap;
    assert_eq!(
        km.handle(&key("F12", Mods::default())),
        vec![EditCommand::GotoDefinition]
    );
    let shift = Mods {
        shift: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("F12", shift)),
        vec![EditCommand::FindReferences]
    );
    let ctrl = Mods {
        ctrl: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key(" ", ctrl)),
        vec![EditCommand::TriggerCompletion]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn cmd_arrow_line_doc_nav_macos() {
    let mut km = VscodeKeymap;
    let cmd = Mods {
        meta: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("ArrowLeft", cmd)),
        vec![EditCommand::Move(Motion::LineStart)]
    );
    assert_eq!(
        km.handle(&key("ArrowRight", cmd)),
        vec![EditCommand::Move(Motion::LineEnd)]
    );
    assert_eq!(
        km.handle(&key("ArrowUp", cmd)),
        vec![EditCommand::Move(Motion::DocStart)]
    );
    assert_eq!(
        km.handle(&key("ArrowDown", cmd)),
        vec![EditCommand::Move(Motion::DocEnd)]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn cmd_shift_arrow_selects_macos() {
    let mut km = VscodeKeymap;
    let cs = Mods {
        meta: true,
        shift: true,
        ..Default::default()
    };
    assert_eq!(
        km.handle(&key("ArrowLeft", cs)),
        vec![EditCommand::Select(Motion::LineStart)]
    );
    assert_eq!(
        km.handle(&key("ArrowDown", cs)),
        vec![EditCommand::Select(Motion::DocEnd)]
    );
}
