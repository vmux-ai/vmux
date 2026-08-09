use super::*;

type U8Listener = GuardedListener<Box<dyn FnMut(u8)>>;
type U8ListenerSlot = Rc<RefCell<Option<U8Listener>>>;

#[test]
fn inactive_listener_ignores_late_events() {
    let count = Rc::new(Cell::new(0));
    let count_for_listener = Rc::clone(&count);
    let listener = GuardedListener::new(move |_: ()| {
        count_for_listener.set(count_for_listener.get() + 1);
    });
    let guard = listener.guard();

    assert!(listener.call(()));
    guard.deactivate();

    assert!(!listener.call(()));
    assert_eq!(count.get(), 1);
}

#[test]
fn borrowed_listener_ignores_reentrant_events() {
    let count = Rc::new(Cell::new(0));
    let holder: U8ListenerSlot = Rc::new(RefCell::new(None));
    let count_for_listener = Rc::clone(&count);
    let holder_for_listener = Rc::clone(&holder);
    let listener = GuardedListener::new(Box::new(move |value| {
        count_for_listener.set(count_for_listener.get() + 1);
        if value == 1u8 {
            let nested = holder_for_listener.borrow();
            assert!(!nested.as_ref().unwrap().call(2));
        }
    }) as Box<dyn FnMut(u8)>);
    *holder.borrow_mut() = Some(listener.clone());

    assert!(listener.call(1));
    assert_eq!(count.get(), 1);
}
