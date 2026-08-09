use super::*;

#[test]
fn snapshot_is_coherent() {
    publish(
        Vec2::new(123.5, 456.25),
        NativeMouseButtons {
            left: true,
            right: false,
            middle: true,
        },
        true,
    );

    let snapshot = snapshot().expect("snapshot");
    assert_eq!(snapshot.position_px, Vec2::new(123.5, 456.25));
    assert_eq!(
        snapshot.buttons,
        NativeMouseButtons {
            left: true,
            right: false,
            middle: true,
        }
    );
    assert_eq!(snapshot.sequence & 1, 0);
    assert_eq!(snapshot.motion_sequence, snapshot.sequence);
}

#[test]
fn button_update_preserves_position_and_motion_sequence() {
    publish(Vec2::new(10.0, 20.0), NativeMouseButtons::default(), true);
    let before = snapshot().expect("snapshot");

    publish_buttons(NativeMouseButtons {
        left: true,
        right: false,
        middle: false,
    });

    let after = snapshot().expect("snapshot");
    assert_eq!(after.position_px, before.position_px);
    assert_eq!(after.motion_sequence, before.motion_sequence);
    assert!(after.buttons.left);
}
