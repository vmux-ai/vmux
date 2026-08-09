use super::*;

#[derive(Debug, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct Ping {
    value: u32,
}

#[derive(Default)]
struct LoopbackHost {
    listeners: RefCell<Vec<(String, BytesListener)>>,
}

impl PageHost for LoopbackHost {
    fn emit(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        for (registered, on_bytes) in self.listeners.borrow_mut().iter_mut() {
            if registered == id {
                on_bytes(bytes);
            }
        }
        Ok(())
    }

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        self.listeners.borrow_mut().push((id.to_string(), on_bytes));
        Ok(())
    }
}

#[test]
fn a_payload_reaches_the_listener_registered_for_its_id() {
    install_host(Rc::new(LoopbackHost::default()));

    let seen = Rc::new(RefCell::new(Vec::<Ping>::new()));
    let sink = seen.clone();
    listen_bytes(
        "ping",
        Box::new(move |bytes| {
            if let Some(ping) = decode_bin_payload::<Ping>(bytes) {
                sink.borrow_mut().push(ping);
            }
        }),
    )
    .unwrap();

    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&Ping { value: 7 }).unwrap();
    emit_bytes("ping", &bytes).unwrap();
    emit_bytes("other", &bytes).unwrap();

    assert_eq!(*seen.borrow(), vec![Ping { value: 7 }]);
}
