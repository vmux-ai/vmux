
export function js_show_toast(header_text, message, level, as_ms) {
    if (typeof showDXToast !== "undefined") {{
        window.showDXToast(header_text, message, level, as_ms);
    }}
}
export function js_schedule_toast(header_text, message, level, as_ms) {
    if (typeof scheduleDXToast !== "undefined") {{
        window.scheduleDXToast(header_text, message, level, as_ms);
    }}
}
