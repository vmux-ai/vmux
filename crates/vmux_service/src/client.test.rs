use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn forwarding_burst_wakes_consumer_once_until_drain() {
    let (tx, rx) = std::sync::mpsc::channel();
    let wakes = Arc::new(AtomicUsize::new(0));
    let wakes_for_callback = Arc::clone(&wakes);
    let wake: ServiceWake = Arc::new(move || {
        wakes_for_callback.fetch_add(1, Ordering::Relaxed);
    });
    let wake_pending = AtomicBool::new(false);

    forward_service_message(
        &tx,
        Some(&wake),
        &wake_pending,
        ServiceMessage::ProcessList {
            processes: Vec::new(),
        },
    )
    .expect("message should forward");
    forward_service_message(
        &tx,
        Some(&wake),
        &wake_pending,
        ServiceMessage::ProcessList {
            processes: Vec::new(),
        },
    )
    .expect("message should forward");

    assert!(matches!(
        rx.try_recv(),
        Ok(ServiceMessage::ProcessList { processes }) if processes.is_empty()
    ));
    assert!(matches!(
        rx.try_recv(),
        Ok(ServiceMessage::ProcessList { processes }) if processes.is_empty()
    ));
    assert_eq!(wakes.load(Ordering::Relaxed), 1);

    wake_pending.store(false, Ordering::Release);
    forward_service_message(
        &tx,
        Some(&wake),
        &wake_pending,
        ServiceMessage::ProcessList {
            processes: Vec::new(),
        },
    )
    .expect("message should forward");

    assert_eq!(wakes.load(Ordering::Relaxed), 2);
}

#[test]
fn service_message_drain_leaves_excess_messages_for_later_frames() {
    let (tx, rx) = std::sync::mpsc::channel();
    for _ in 0..=MAX_SERVICE_MESSAGES_PER_DRAIN {
        tx.send(ServiceMessage::ProcessList {
            processes: Vec::new(),
        })
        .expect("service message should queue");
    }

    let (drained, capped) = drain_service_messages_bounded(&rx);

    assert_eq!(drained.len(), MAX_SERVICE_MESSAGES_PER_DRAIN);
    assert!(
        capped,
        "hitting the cap must report capped so the caller re-wakes"
    );
    assert!(rx.try_recv().is_ok());
}

#[test]
fn service_message_drain_reports_not_capped_when_drained_dry() {
    let (tx, rx) = std::sync::mpsc::channel();
    for _ in 0..3 {
        tx.send(ServiceMessage::ProcessList {
            processes: Vec::new(),
        })
        .expect("service message should queue");
    }

    let (drained, capped) = drain_service_messages_bounded(&rx);

    assert_eq!(drained.len(), 3);
    assert!(!capped);
    assert!(rx.try_recv().is_err());
}
