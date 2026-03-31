//! JS bridge: clock ticks + `window.cef.listen("vmux_status", …)` via `document::eval`.
//!
//! Each `dioxus.send` carries a **RON** document as a JSON string on the wire (Dioxus deserializes to
//! [`String`]); Rust parses with `ron::from_str` into [`BridgeMsg`](crate::payload::BridgeMsg).

/// Injected into the page so WASM can `recv` RON messages (as strings) from JS.
pub const EVAL_SCRIPT: &str = r#"
    function pad(n) { return n < 10 ? "0" + n : String(n); }
    function monthShort(m) {
        return ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"][m] || "";
    }
    function clockStr() {
        var d = new Date();
        var w = ["Sun","Mon","Tue","Wed","Thu","Fri","Sat"][d.getDay()] || "";
        return w + " " + monthShort(d.getMonth()) + " " + d.getDate() + " "
            + pad(d.getHours()) + ":" + pad(d.getMinutes()) + ":" + pad(d.getSeconds());
    }
    function ronClock() {
        return "(Clock(text: " + JSON.stringify(clockStr()) + "))";
    }
    setInterval(function() {
        dioxus.send(ronClock());
    }, 1000);
    dioxus.send(ronClock());
    try {
        if (window.cef && typeof window.cef.listen === "function") {
            window.cef.listen("vmux_status", function (e) {
                var json = JSON.stringify(e);
                dioxus.send("(Status(payload: " + JSON.stringify(json) + "))");
            });
        }
    } catch (_) {}
"#;
