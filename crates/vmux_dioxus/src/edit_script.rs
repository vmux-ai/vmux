//! Delivering a batch to a page that can only be reached by evaluating a script.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// A batch of edits, wrapped as JavaScript for a host that has `evaluate_script` and nothing else.
pub struct EditScript(String);

impl EditScript {
    pub fn of(edits: &[u8]) -> Self {
        // Base64's alphabet is `A-Za-z0-9+/=`, so the encoded batch cannot close the string
        // literal it is interpolated into and needs no further escaping.
        let encoded = STANDARD.encode(edits);

        // `run_from_bytes`, not `rafEdits`: the latter ends in `markEditsFinished`, which sends an
        // acknowledgement over the WebSocket that dioxus-desktop opens and this crate does not, so
        // it would throw on `this.edits` being undefined.
        Self(format!(
            r#"(function(){{
const binary = atob("{encoded}");
const bytes = new Uint8Array(binary.length);
for (let i = 0; i < binary.length; i++) {{ bytes[i] = binary.charCodeAt(i); }}
window.interpreter.run_from_bytes(bytes.buffer);
}})();"#
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_batch_survives_the_round_trip_the_script_performs() {
        let edits: Vec<u8> = (0u8..=255).collect();
        let script = EditScript::of(&edits);

        let encoded = script
            .as_str()
            .split_once(r#"atob(""#)
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(encoded, _)| encoded)
            .expect("the script decodes a base64 literal");

        assert_eq!(
            STANDARD.decode(encoded).expect("valid base64"),
            edits,
            "every byte value has to reach the interpreter unaltered"
        );
    }

    #[test]
    fn the_script_does_not_acknowledge_over_a_socket_that_was_never_opened() {
        let script = EditScript::of(&[1, 2, 3]);

        assert!(script.as_str().contains("run_from_bytes"));
        assert!(
            !script.as_str().contains("rafEdits"),
            "rafEdits calls markEditsFinished, which writes to the WebSocket only dioxus-desktop opens"
        );
    }
}
