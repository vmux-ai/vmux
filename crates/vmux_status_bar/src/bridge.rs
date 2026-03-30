//! JS bridge: clock ticks + `window.cef.listen("vmux_status", …)` via `document::eval`.

/// Injected into the page so WASM can `recv` JSON messages from JS.
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
    setInterval(function() {
        dioxus.send({ type: "clock", text: clockStr() });
    }, 1000);
    dioxus.send({ type: "clock", text: clockStr() });
    try {
        if (window.cef && typeof window.cef.listen === "function") {
            window.cef.listen("vmux_status", function (e) {
                dioxus.send({ type: "status", payload: e });
            });
        }
    } catch (_) {}
"#;
