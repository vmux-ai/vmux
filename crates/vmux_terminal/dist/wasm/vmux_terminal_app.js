//#region js imports
import { getMetaContents } from './snippets/dioxus-cli-config-124b3ddf2574706e/inline0.js';
import { RawInterpreter } from './snippets/dioxus-interpreter-js-cee195cd562b7a49/inline0.js';
import { setAttributeInner } from './snippets/dioxus-interpreter-js-cee195cd562b7a49/src/js/set_attribute.js';
import { js_schedule_toast, js_show_toast } from './snippets/dioxus-web-579409375adb7471/inline0.js';
import { get_select_data } from './snippets/dioxus-web-579409375adb7471/inline1.js';
import { WebDioxusChannel } from './snippets/dioxus-web-579409375adb7471/src/js/eval.js';

//#endregion

//#region exports

export class JSOwner {
    constructor() {
        throw new Error('cannot invoke `new` directly');
    }
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(JSOwner.prototype);
        obj.__wbg_ptr = ptr;
        JSOwnerFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JSOwnerFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jsowner_free(ptr, 0);
    }
}
if (Symbol.dispose) JSOwner.prototype[Symbol.dispose] = JSOwner.prototype.free;

//#endregion

//#region wasm imports
import * as import1 from "./snippets/dioxus-interpreter-js-cee195cd562b7a49/src/js/patch_console.js"

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_55538483de6e3abe: function() { return logError(function (arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_String_8564e559799eccda: function() { return logError(function (arg0, arg1) {
            const ret = String(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg___wbindgen_bigint_get_as_i64_a738e80c0fe6f6a7: function(arg0, arg1) {
            const v = arg1;
            const ret = typeof(v) === 'bigint' ? v : undefined;
            if (!isLikeNone(ret)) {
                _assertBigInt(ret);
            }
            getDataViewMemory0().setBigInt64(arg0 + 8 * 1, isLikeNone(ret) ? BigInt(0) : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_boolean_get_fe2a24fdfdb4064f: function(arg0) {
            const v = arg0;
            const ret = typeof(v) === 'boolean' ? v : undefined;
            if (!isLikeNone(ret)) {
                _assertBoolean(ret);
            }
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_debug_string_d89627202d0155b7: function(arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_exports_8d9810169ed188c3: function() {
            const ret = wasm;
            return ret;
        },
        __wbg___wbindgen_function_table_0eacdfca1b824e47: function() {
            const ret = wasm.__wbindgen_export;
            return ret;
        },
        __wbg___wbindgen_in_fe3eb6a509f75744: function(arg0, arg1) {
            const ret = arg0 in arg1;
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_is_bigint_ca270ac12ef71091: function(arg0) {
            const ret = typeof(arg0) === 'bigint';
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_is_function_2a95406423ea8626: function(arg0) {
            const ret = typeof(arg0) === 'function';
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_is_null_8d90524c9e0af183: function(arg0) {
            const ret = arg0 === null;
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_is_object_59a002e76b059312: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_is_string_624d5244bb2bc87c: function(arg0) {
            const ret = typeof(arg0) === 'string';
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_is_undefined_87a3a837f331fef5: function(arg0) {
            const ret = arg0 === undefined;
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_jsval_eq_eedd705f9f2a4f35: function(arg0, arg1) {
            const ret = arg0 === arg1;
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_jsval_loose_eq_cf851f110c48f9ba: function(arg0, arg1) {
            const ret = arg0 == arg1;
            _assertBoolean(ret);
            return ret;
        },
        __wbg___wbindgen_memory_625f3ce5935b6a99: function() {
            const ret = wasm.memory;
            return ret;
        },
        __wbg___wbindgen_number_get_769f3676dc20c1d7: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            if (!isLikeNone(ret)) {
                _assertNum(ret);
            }
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_f1161390414f9b59: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_5549492daedad139: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg__wbg_cb_unref_fbe69bb076c16bad: function() { return logError(function (arg0) {
            arg0._wbg_cb_unref();
        }, arguments); },
        __wbg_addEventListener_ee34fcb181ae85b2: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_altKey_24c984d8a53e7288: function() { return logError(function (arg0) {
            const ret = arg0.altKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_altKey_e4bf8ddc09ccb4a0: function() { return logError(function (arg0) {
            const ret = arg0.altKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_altKey_fbd30c4040122f9a: function() { return logError(function (arg0) {
            const ret = arg0.altKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_animationName_15b60a04b4ef21f1: function() { return logError(function (arg0, arg1) {
            const ret = arg1.animationName;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_appendChild_79e0048b1f1b7921: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.appendChild(arg1);
            return ret;
        }, arguments); },
        __wbg_arrayBuffer_9f258d017f7107c5: function() { return handleError(function (arg0) {
            const ret = arg0.arrayBuffer();
            return ret;
        }, arguments); },
        __wbg_arrayBuffer_f64a4dc47790c6db: function() { return logError(function (arg0) {
            const ret = arg0.arrayBuffer();
            return ret;
        }, arguments); },
        __wbg_back_e16cd8cc112bd5a8: function() { return handleError(function (arg0) {
            arg0.back();
        }, arguments); },
        __wbg_blockSize_f3c49edc5f00fb61: function() { return logError(function (arg0) {
            const ret = arg0.blockSize;
            return ret;
        }, arguments); },
        __wbg_blur_8d0d46d8f6b9f21d: function() { return handleError(function (arg0) {
            arg0.blur();
        }, arguments); },
        __wbg_borderBoxSize_18ce68694ee753c1: function() { return logError(function (arg0) {
            const ret = arg0.borderBoxSize;
            return ret;
        }, arguments); },
        __wbg_boundingClientRect_eca5cbd383f7ab49: function() { return logError(function (arg0) {
            const ret = arg0.boundingClientRect;
            return ret;
        }, arguments); },
        __wbg_bubbles_eb5e5d845d178a31: function() { return logError(function (arg0) {
            const ret = arg0.bubbles;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_buffer_efacb895ff4aaac2: function() { return logError(function (arg0) {
            const ret = arg0.buffer;
            return ret;
        }, arguments); },
        __wbg_button_93558e5e0c047d36: function() { return logError(function (arg0) {
            const ret = arg0.button;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_buttons_9e629dc8d196c20f: function() { return logError(function (arg0) {
            const ret = arg0.buttons;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_byteLength_1a2df3aed80893d6: function() { return logError(function (arg0) {
            const ret = arg0.byteLength;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_call_4f2f92601568b772: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg0.call(arg1, arg2, arg3);
            return ret;
        }, arguments); },
        __wbg_call_6ae20895a60069a2: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.call(arg1);
            return ret;
        }, arguments); },
        __wbg_call_8f5d7bb070283508: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_changedTouches_66d1b01442bf6cb4: function() { return logError(function (arg0) {
            const ret = arg0.changedTouches;
            return ret;
        }, arguments); },
        __wbg_charCodeAt_49f61ffe0ac23535: function() { return logError(function (arg0, arg1) {
            const ret = arg0.charCodeAt(arg1 >>> 0);
            return ret;
        }, arguments); },
        __wbg_checkValidity_b134241f11dab1c8: function() { return logError(function (arg0) {
            const ret = arg0.checkValidity();
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_checked_e649ee8502392435: function() { return logError(function (arg0) {
            const ret = arg0.checked;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_clearData_44d1c600c7cd1000: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.clearData(getStringFromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_clearData_f780c8846a36f824: function() { return handleError(function (arg0) {
            arg0.clearData();
        }, arguments); },
        __wbg_clearTimeout_113b1cde814ec762: function() { return logError(function (arg0) {
            const ret = clearTimeout(arg0);
            return ret;
        }, arguments); },
        __wbg_clientHeight_a088c0a046303c1e: function() { return logError(function (arg0) {
            const ret = arg0.clientHeight;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_clientWidth_489775f176e76aa6: function() { return logError(function (arg0) {
            const ret = arg0.clientWidth;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_clientX_4136e615bf7e2a7c: function() { return logError(function (arg0) {
            const ret = arg0.clientX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_clientX_ebc7e3f31a9492bd: function() { return logError(function (arg0) {
            const ret = arg0.clientX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_clientY_26dcab94f3942474: function() { return logError(function (arg0) {
            const ret = arg0.clientY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_clientY_c39d37c3a25ed988: function() { return logError(function (arg0) {
            const ret = arg0.clientY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_code_5a9172aa3b02dba5: function() { return logError(function (arg0, arg1) {
            const ret = arg1.code;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_code_7eb5b8af0cea9f25: function() { return logError(function (arg0) {
            const ret = arg0.code;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_contentBoxSize_9ddbac612c428f26: function() { return logError(function (arg0) {
            const ret = arg0.contentBoxSize;
            return ret;
        }, arguments); },
        __wbg_createComment_92442ea9e7416a8a: function() { return logError(function (arg0, arg1, arg2) {
            const ret = arg0.createComment(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_createElementNS_f3714b3aee0a62f3: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            const ret = arg0.createElementNS(arg1 === 0 ? undefined : getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
            return ret;
        }, arguments); },
        __wbg_createElement_a8dcfa25dbf80c51: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.createElement(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_createTask_2e55ab0673ae81d8: function() { return handleError(function (arg0, arg1) {
            const ret = console.createTask(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_createTextNode_9d7d2fed8d685efa: function() { return logError(function (arg0, arg1, arg2) {
            const ret = arg0.createTextNode(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_ctrlKey_81931b7aea899650: function() { return logError(function (arg0) {
            const ret = arg0.ctrlKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_ctrlKey_b5568c14a165d27f: function() { return logError(function (arg0) {
            const ret = arg0.ctrlKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_ctrlKey_b6bedeb29e4933b6: function() { return logError(function (arg0) {
            const ret = arg0.ctrlKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_dataTransfer_89aea0b8352d3dd3: function() { return logError(function (arg0) {
            const ret = arg0.dataTransfer;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_data_1921ba515ab95fc3: function() { return logError(function (arg0, arg1) {
            const ret = arg1.data;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_data_7de671a92a650aba: function() { return logError(function (arg0) {
            const ret = arg0.data;
            return ret;
        }, arguments); },
        __wbg_deltaMode_1537200199f699ee: function() { return logError(function (arg0) {
            const ret = arg0.deltaMode;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_deltaX_8dfdf13c51b3b146: function() { return logError(function (arg0) {
            const ret = arg0.deltaX;
            return ret;
        }, arguments); },
        __wbg_deltaY_3a0ecaf2f6da7352: function() { return logError(function (arg0) {
            const ret = arg0.deltaY;
            return ret;
        }, arguments); },
        __wbg_deltaZ_ac208d9568a1f362: function() { return logError(function (arg0) {
            const ret = arg0.deltaZ;
            return ret;
        }, arguments); },
        __wbg_detail_bfefa4405dbef817: function() { return logError(function (arg0) {
            const ret = arg0.detail;
            return ret;
        }, arguments); },
        __wbg_documentElement_c375315afd6a4740: function() { return logError(function (arg0) {
            const ret = arg0.documentElement;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_document_cf512e4e2300751d: function() { return logError(function (arg0) {
            const ret = arg0.document;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_done_19f92cb1f8738aba: function() { return logError(function (arg0) {
            const ret = arg0.done;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_dropEffect_20194db662a01b15: function() { return logError(function (arg0, arg1) {
            const ret = arg1.dropEffect;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_effectAllowed_c7fb1e6cc0449475: function() { return logError(function (arg0, arg1) {
            const ret = arg1.effectAllowed;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_elapsedTime_3604c1a891a337c2: function() { return logError(function (arg0) {
            const ret = arg0.elapsedTime;
            return ret;
        }, arguments); },
        __wbg_elapsedTime_98de691ac17d7740: function() { return logError(function (arg0) {
            const ret = arg0.elapsedTime;
            return ret;
        }, arguments); },
        __wbg_entries_28ed7cb892e12eff: function() { return logError(function (arg0) {
            const ret = Object.entries(arg0);
            return ret;
        }, arguments); },
        __wbg_entries_6ffdad39537c5176: function() { return logError(function (arg0) {
            const ret = arg0.entries();
            return ret;
        }, arguments); },
        __wbg_error_d1ac665121096249: function() { return logError(function (arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        }, arguments); },
        __wbg_error_de6b86e598505246: function() { return logError(function (arg0) {
            console.error(arg0);
        }, arguments); },
        __wbg_fetch_cf916c4e32b4bb5e: function() { return logError(function (arg0, arg1, arg2) {
            const ret = arg0.fetch(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_files_3b330f17d6c5f92d: function() { return logError(function (arg0) {
            const ret = arg0.files;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_files_dad88a18a0646065: function() { return logError(function (arg0) {
            const ret = arg0.files;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_focus_7202f35a804d3965: function() { return handleError(function (arg0) {
            arg0.focus();
        }, arguments); },
        __wbg_force_5d6a155d143a3703: function() { return logError(function (arg0) {
            const ret = arg0.force;
            return ret;
        }, arguments); },
        __wbg_forward_bf76efde0204c2b2: function() { return handleError(function (arg0) {
            arg0.forward();
        }, arguments); },
        __wbg_getAsFile_53a3509ae11194f9: function() { return handleError(function (arg0) {
            const ret = arg0.getAsFile();
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_getAttribute_94c78997e59ce800: function() { return logError(function (arg0, arg1, arg2, arg3) {
            const ret = arg1.getAttribute(getStringFromWasm0(arg2, arg3));
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getBoundingClientRect_a4588c60b4b781a4: function() { return logError(function (arg0) {
            const ret = arg0.getBoundingClientRect();
            return ret;
        }, arguments); },
        __wbg_getData_ec6498e981620fc7: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg1.getData(getStringFromWasm0(arg2, arg3));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getElementById_7c22ba050c51a610: function() { return logError(function (arg0, arg1, arg2) {
            const ret = arg0.getElementById(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_getMetaContents_8fe242db88ed3eae: function() { return logError(function (arg0, arg1, arg2) {
            const ret = getMetaContents(getStringFromWasm0(arg1, arg2));
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getNode_435d0f2295da1343: function() { return logError(function (arg0, arg1) {
            const ret = arg0.getNode(arg1 >>> 0);
            return ret;
        }, arguments); },
        __wbg_get_6eadbda4395687cb: function() { return logError(function (arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_get_94f5fc088edd3138: function() { return logError(function (arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        }, arguments); },
        __wbg_get_a50328e7325d7f9b: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_acfcf2a0b40744f2: function() { return logError(function (arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_get_af60fd82a84e517c: function() { return logError(function (arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_get_ff5f1fb220233477: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_select_data_7b193d2dffb5edd9: function() { return logError(function (arg0, arg1) {
            const ret = get_select_data(arg1);
            const ptr1 = passArrayJsValueToWasm0(ret, wasm.__wbindgen_malloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_get_unchecked_7c6bbabf5b0b1fbf: function() { return logError(function (arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        }, arguments); },
        __wbg_grow_681599f2609bfe8c: function() { return logError(function (arg0, arg1) {
            const ret = arg0.grow(arg1 >>> 0);
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_grow_7183a30e070bd0f5: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.grow(arg1 >>> 0);
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_hash_7ae903ee70c3dd41: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.hash;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_head_d83cbfe2b8863e8d: function() { return logError(function (arg0) {
            const ret = arg0.head;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_height_3b3cf8c3fb90ba70: function() { return logError(function (arg0) {
            const ret = arg0.height;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_height_6324f7f0eb2ef72c: function() { return logError(function (arg0) {
            const ret = arg0.height;
            return ret;
        }, arguments); },
        __wbg_height_a030a387b18d604b: function() { return logError(function (arg0) {
            const ret = arg0.height;
            return ret;
        }, arguments); },
        __wbg_history_304baab31767c8be: function() { return handleError(function (arg0) {
            const ret = arg0.history;
            return ret;
        }, arguments); },
        __wbg_host_fdcb41d66cd3e943: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.host;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_identifier_5865d3633dd5b8f7: function() { return logError(function (arg0) {
            const ret = arg0.identifier;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_initialize_25517013d60acd13: function() { return logError(function (arg0, arg1, arg2) {
            arg0.initialize(arg1, arg2);
        }, arguments); },
        __wbg_inlineSize_9314da5648ee8510: function() { return logError(function (arg0) {
            const ret = arg0.inlineSize;
            return ret;
        }, arguments); },
        __wbg_instanceof_ArrayBuffer_8d855993947fc3a2: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof ArrayBuffer;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_Blob_0ba6040bc29f038a: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Blob;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_Document_3b5716956bb1cdc4: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Document;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_DragEvent_40e7b30c164d033e: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof DragEvent;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_Element_60d1572f735fb2ce: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Element;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_File_df82313f164d96c6: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof File;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_HtmlElement_78785ade06923935: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLElement;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_HtmlFormElement_b4d622fffe6f32d9: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLFormElement;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_HtmlInputElement_b9b28a72ccf030aa: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLInputElement;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_HtmlSelectElement_cc51eb13c39fb68c: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLSelectElement;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_HtmlTextAreaElement_4375de8afea2fedf: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof HTMLTextAreaElement;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_Map_238410f1463c05ed: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Map;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_Node_c72895807e8b83e6: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Node;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_Uint8Array_ce24d58a5f4bdcc3: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instanceof_Window_2fa8d9c2d5b6104a: function() { return logError(function (arg0) {
            let result;
            try {
                result = arg0 instanceof Window;
            } catch (_) {
                result = false;
            }
            const ret = result;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_instantiate_146195515f351d2b: function() { return logError(function (arg0, arg1) {
            const ret = WebAssembly.instantiate(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_intersectionRatio_026bad35aaed1171: function() { return logError(function (arg0) {
            const ret = arg0.intersectionRatio;
            return ret;
        }, arguments); },
        __wbg_intersectionRect_f55eca6a1b6b8c82: function() { return logError(function (arg0) {
            const ret = arg0.intersectionRect;
            return ret;
        }, arguments); },
        __wbg_isArray_867202cf8f195ed8: function() { return logError(function (arg0) {
            const ret = Array.isArray(arg0);
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_isComposing_550ebe370ecf7df5: function() { return logError(function (arg0) {
            const ret = arg0.isComposing;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_isIntersecting_e320c08ee1151758: function() { return logError(function (arg0) {
            const ret = arg0.isIntersecting;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_isPrimary_25761537a10a3792: function() { return logError(function (arg0) {
            const ret = arg0.isPrimary;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_isSafeInteger_1dfae065cbfe1915: function() { return logError(function (arg0) {
            const ret = Number.isSafeInteger(arg0);
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_item_7da69d673a9dbfc8: function() { return logError(function (arg0, arg1) {
            const ret = arg0.item(arg1 >>> 0);
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_items_5776fda85ca5b790: function() { return logError(function (arg0) {
            const ret = arg0.items;
            return ret;
        }, arguments); },
        __wbg_iterator_54661826e186eb6a: function() { return logError(function () {
            const ret = Symbol.iterator;
            return ret;
        }, arguments); },
        __wbg_js_schedule_toast_9a30ee06123c81db: function() { return logError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg4;
                deferred0_1 = arg5;
                js_schedule_toast(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3), getStringFromWasm0(arg4, arg5), arg6 >>> 0);
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        }, arguments); },
        __wbg_js_show_toast_8355f58929af657f: function() { return logError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg4;
                deferred0_1 = arg5;
                js_show_toast(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3), getStringFromWasm0(arg4, arg5), arg6 >>> 0);
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        }, arguments); },
        __wbg_key_75d93864da2a4ec7: function() { return logError(function (arg0, arg1) {
            const ret = arg1.key;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_keys_e84d806594765111: function() { return logError(function (arg0) {
            const ret = Object.keys(arg0);
            return ret;
        }, arguments); },
        __wbg_kind_c528ab8e5dc1a9e5: function() { return logError(function (arg0, arg1) {
            const ret = arg1.kind;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_lastModified_0c7f254cd96f6c93: function() { return logError(function (arg0) {
            const ret = arg0.lastModified;
            return ret;
        }, arguments); },
        __wbg_left_3b637e6527576a90: function() { return logError(function (arg0) {
            const ret = arg0.left;
            return ret;
        }, arguments); },
        __wbg_length_018f80fad9730468: function() { return logError(function (arg0) {
            const ret = arg0.length;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_length_85dfb54ae1d3ef8d: function() { return logError(function (arg0) {
            const ret = arg0.length;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_length_a4213c6b847ad288: function() { return logError(function (arg0) {
            const ret = arg0.length;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_length_b6cd0a4006591e8e: function() { return logError(function (arg0) {
            const ret = arg0.length;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_length_dbcde84c72f9a3f3: function() { return logError(function (arg0) {
            const ret = arg0.length;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_length_e6e1633fbea6cfa9: function() { return logError(function (arg0) {
            const ret = arg0.length;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_length_fae3e439140f48a4: function() { return logError(function (arg0) {
            const ret = arg0.length;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_location_639bffbf5fe10651: function() { return logError(function (arg0) {
            const ret = arg0.location;
            return ret;
        }, arguments); },
        __wbg_location_b63d18fa57e80754: function() { return logError(function (arg0) {
            const ret = arg0.location;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_log_0c201ade58bb55e1: function() { return logError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.log(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3), getStringFromWasm0(arg4, arg5), getStringFromWasm0(arg6, arg7));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        }, arguments); },
        __wbg_log_ce2c4456b290c5e7: function() { return logError(function (arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.log(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        }, arguments); },
        __wbg_mark_b4d943f3bc2d2404: function() { return logError(function (arg0, arg1) {
            performance.mark(getStringFromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_measure_84362959e621a2c1: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            let deferred0_0;
            let deferred0_1;
            let deferred1_0;
            let deferred1_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                deferred1_0 = arg2;
                deferred1_1 = arg3;
                performance.measure(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
                wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
            }
        }, arguments); },
        __wbg_metaKey_30ab79cab4fa8ac1: function() { return logError(function (arg0) {
            const ret = arg0.metaKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_metaKey_51360b2615b12218: function() { return logError(function (arg0) {
            const ret = arg0.metaKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_metaKey_5ad20c9c8d673f9c: function() { return logError(function (arg0) {
            const ret = arg0.metaKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_name_0eb19969dfedae68: function() { return logError(function (arg0, arg1) {
            const ret = arg1.name;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_name_13eb211eafa5eae9: function() { return logError(function (arg0, arg1) {
            const ret = arg1.name;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_new_01d3631e39c44ed7: function() { return handleError(function (arg0, arg1) {
            const ret = new WebAssembly.Global(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_new_0934b88171ef61b0: function() { return logError(function () {
            const ret = new Map();
            return ret;
        }, arguments); },
        __wbg_new_09855e3e9e8e3a5f: function() { return handleError(function () {
            const ret = new DataTransfer();
            return ret;
        }, arguments); },
        __wbg_new_1d96678aaacca32e: function() { return logError(function (arg0) {
            const ret = new Uint8Array(arg0);
            return ret;
        }, arguments); },
        __wbg_new_4370be21fa2b2f80: function() { return logError(function () {
            const ret = new Array();
            return ret;
        }, arguments); },
        __wbg_new_48e1d86cfd30c8e7: function() { return logError(function () {
            const ret = new Object();
            return ret;
        }, arguments); },
        __wbg_new_69642b0f6c3151cc: function() { return handleError(function (arg0, arg1) {
            const ret = new WebSocket(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_new_6b07bc4ea020361d: function() { return logError(function (arg0) {
            const ret = new WebDioxusChannel(JSOwner.__wrap(arg0));
            return ret;
        }, arguments); },
        __wbg_new_bb3b78afe153c98c: function() { return handleError(function () {
            const ret = new FileReader();
            return ret;
        }, arguments); },
        __wbg_new_c0073c15d328c374: function() { return logError(function (arg0) {
            const ret = new RawInterpreter(arg0 >>> 0);
            return ret;
        }, arguments); },
        __wbg_new_f17c87d2167f1b51: function() { return logError(function () {
            const ret = new Error();
            return ret;
        }, arguments); },
        __wbg_new_with_args_8d3386a3e4c28c2b: function() { return logError(function (arg0, arg1, arg2, arg3) {
            const ret = new Function(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3));
            return ret;
        }, arguments); },
        __wbg_new_with_form_8350eb12462bd8ef: function() { return handleError(function (arg0) {
            const ret = new FormData(arg0);
            return ret;
        }, arguments); },
        __wbg_next_55d835fe0ab5b3e7: function() { return logError(function (arg0) {
            const ret = arg0.next;
            return ret;
        }, arguments); },
        __wbg_next_e34cfb9df1518d7c: function() { return handleError(function (arg0) {
            const ret = arg0.next();
            return ret;
        }, arguments); },
        __wbg_offsetX_57cce2d8ea8bc837: function() { return logError(function (arg0) {
            const ret = arg0.offsetX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_offsetY_50fccc49deb4f3d6: function() { return logError(function (arg0) {
            const ret = arg0.offsetY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_ok_5865d0a94e185135: function() { return logError(function (arg0) {
            const ret = arg0.ok;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_ownerDocument_79f10526e0a993b5: function() { return logError(function (arg0) {
            const ret = arg0.ownerDocument;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_pageX_825f81e5926e24e0: function() { return logError(function (arg0) {
            const ret = arg0.pageX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_pageX_c9a96a09ab9ae322: function() { return logError(function (arg0) {
            const ret = arg0.pageX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_pageY_495fa02b357e9a9f: function() { return logError(function (arg0) {
            const ret = arg0.pageY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_pageY_cdcb0645e3dac917: function() { return logError(function (arg0) {
            const ret = arg0.pageY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_parentElement_90ebc389e4feec98: function() { return logError(function (arg0) {
            const ret = arg0.parentElement;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_pathname_f1d35181cabf06f5: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.pathname;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_pointerId_ee9c808306f8c150: function() { return logError(function (arg0) {
            const ret = arg0.pointerId;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_pointerType_0d6bf374e3fe2347: function() { return logError(function (arg0, arg1) {
            const ret = arg1.pointerType;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_pressure_d1f2cd1c42d2c7cd: function() { return logError(function (arg0) {
            const ret = arg0.pressure;
            return ret;
        }, arguments); },
        __wbg_preventDefault_a85164025505c2f9: function() { return logError(function (arg0) {
            arg0.preventDefault();
        }, arguments); },
        __wbg_propertyName_f994b80e0ae02975: function() { return logError(function (arg0, arg1) {
            const ret = arg1.propertyName;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_protocol_19d44b8be55114d9: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.protocol;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_prototypesetcall_3875d54d12ef2eec: function() { return logError(function (arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        }, arguments); },
        __wbg_pseudoElement_38b2ef471c8eef17: function() { return logError(function (arg0, arg1) {
            const ret = arg1.pseudoElement;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_pseudoElement_c338178f0f55d034: function() { return logError(function (arg0, arg1) {
            const ret = arg1.pseudoElement;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_pushState_e06769078a41fe94: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.pushState(arg1, getStringFromWasm0(arg2, arg3), arg4 === 0 ? undefined : getStringFromWasm0(arg4, arg5));
        }, arguments); },
        __wbg_push_d0006a37f9fcda6d: function() { return logError(function (arg0, arg1) {
            const ret = arg0.push(arg1);
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_querySelectorAll_57323467181fd36f: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.querySelectorAll(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_queueMicrotask_8868365114fe23b5: function() { return logError(function (arg0) {
            queueMicrotask(arg0);
        }, arguments); },
        __wbg_queueMicrotask_cfc5a0e62f9ebdbe: function() { return logError(function (arg0) {
            const ret = arg0.queueMicrotask;
            return ret;
        }, arguments); },
        __wbg_radiusX_43286951e0a314dc: function() { return logError(function (arg0) {
            const ret = arg0.radiusX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_radiusY_1bd4ec1266a54c1a: function() { return logError(function (arg0) {
            const ret = arg0.radiusY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_random_09b0bd71e83551d7: function() { return logError(function () {
            const ret = Math.random();
            return ret;
        }, arguments); },
        __wbg_readAsArrayBuffer_2216fc49399813a2: function() { return handleError(function (arg0, arg1) {
            arg0.readAsArrayBuffer(arg1);
        }, arguments); },
        __wbg_readAsText_299f8dabc5b3ebda: function() { return handleError(function (arg0, arg1) {
            arg0.readAsText(arg1);
        }, arguments); },
        __wbg_reload_912a27dc75420862: function() { return handleError(function (arg0) {
            arg0.reload();
        }, arguments); },
        __wbg_repeat_547533b6c196665b: function() { return logError(function (arg0) {
            const ret = arg0.repeat;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_replaceState_ac4502ca173046c9: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.replaceState(arg1, getStringFromWasm0(arg2, arg3), arg4 === 0 ? undefined : getStringFromWasm0(arg4, arg5));
        }, arguments); },
        __wbg_requestAnimationFrame_3b4101343dfc745b: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.requestAnimationFrame(arg1);
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_resolve_d8059bc113e215bf: function() { return logError(function (arg0) {
            const ret = Promise.resolve(arg0);
            return ret;
        }, arguments); },
        __wbg_result_0331a88f2582581b: function() { return handleError(function (arg0) {
            const ret = arg0.result;
            return ret;
        }, arguments); },
        __wbg_rootBounds_ebcb0de78a419113: function() { return logError(function (arg0) {
            const ret = arg0.rootBounds;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_rotationAngle_5d99bbec7eeb3040: function() { return logError(function (arg0) {
            const ret = arg0.rotationAngle;
            return ret;
        }, arguments); },
        __wbg_run_48cbe3a4cd27dc82: function() { return logError(function (arg0) {
            arg0.run();
        }, arguments); },
        __wbg_run_73e1363df4d8e653: function() { return logError(function (arg0, arg1, arg2) {
            try {
                var state0 = {a: arg1, b: arg2};
                var cb0 = () => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return _ZN12wasm_bindgen7convert8closures1_6invoke17h8176746aa406cec1E(a, state0.b, );
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = arg0.run(cb0);
                _assertBoolean(ret);
                return ret;
            } finally {
                state0.a = 0;
            }
        }, arguments); },
        __wbg_rustRecv_39526bcf3b078b2f: function() { return logError(function (arg0) {
            const ret = arg0.rustRecv();
            return ret;
        }, arguments); },
        __wbg_rustSend_f5d4eaaf39a73437: function() { return logError(function (arg0, arg1) {
            arg0.rustSend(arg1);
        }, arguments); },
        __wbg_saveTemplate_6c774d0dcd3d2e0e: function() { return logError(function (arg0, arg1, arg2, arg3) {
            var v0 = getArrayJsValueFromWasm0(arg1, arg2).slice();
            wasm.__wbindgen_free(arg1, arg2 * 4, 4);
            arg0.saveTemplate(v0, arg3);
        }, arguments); },
        __wbg_screenX_6597d16da39e9d5b: function() { return logError(function (arg0) {
            const ret = arg0.screenX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_screenX_b8665da9b89e2c52: function() { return logError(function (arg0) {
            const ret = arg0.screenX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_screenY_0dd7897d076570fb: function() { return logError(function (arg0) {
            const ret = arg0.screenY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_screenY_e24f80b664658707: function() { return logError(function (arg0) {
            const ret = arg0.screenY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_scrollHeight_826422356740f5c6: function() { return logError(function (arg0) {
            const ret = arg0.scrollHeight;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_scrollIntoView_a34f4da9dd286848: function() { return logError(function (arg0, arg1) {
            arg0.scrollIntoView(arg1);
        }, arguments); },
        __wbg_scrollLeft_5f3950752da7f3b6: function() { return logError(function (arg0) {
            const ret = arg0.scrollLeft;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_scrollTo_78f05db9ee2df5e7: function() { return logError(function (arg0, arg1, arg2) {
            arg0.scrollTo(arg1, arg2);
        }, arguments); },
        __wbg_scrollTop_cf925e21bf7a9e84: function() { return logError(function (arg0) {
            const ret = arg0.scrollTop;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_scrollWidth_d8de167e744f7475: function() { return logError(function (arg0) {
            const ret = arg0.scrollWidth;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_scrollX_605afd8896a8cba5: function() { return handleError(function (arg0) {
            const ret = arg0.scrollX;
            return ret;
        }, arguments); },
        __wbg_scrollY_77bc9855cdc48959: function() { return handleError(function (arg0) {
            const ret = arg0.scrollY;
            return ret;
        }, arguments); },
        __wbg_scroll_0f56665bcd35a1f2: function() { return logError(function (arg0, arg1) {
            arg0.scroll(arg1);
        }, arguments); },
        __wbg_search_0da984ff6207e002: function() { return handleError(function (arg0, arg1) {
            const ret = arg1.search;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_setAttributeInner_9430738c4b3a6ea0: function() { return logError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            setAttributeInner(arg0, getStringFromWasm0(arg1, arg2), arg3, arg4 === 0 ? undefined : getStringFromWasm0(arg4, arg5));
        }, arguments); },
        __wbg_setAttribute_981e9c312a132ede: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setAttribute(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_setData_8e906f4a6ab48d5c: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setData(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_setTimeout_c1c9a18b6343ebd3: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.setTimeout(arg1, arg2);
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_setTimeout_ef24d2fc3ad97385: function() { return handleError(function (arg0, arg1) {
            const ret = setTimeout(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_set_4702dfa37c77f492: function() { return logError(function (arg0, arg1, arg2) {
            arg0[arg1 >>> 0] = arg2;
        }, arguments); },
        __wbg_set_6be42768c690e380: function() { return logError(function (arg0, arg1, arg2) {
            arg0[arg1] = arg2;
        }, arguments); },
        __wbg_set_8c6629931852a4a5: function() { return logError(function (arg0, arg1, arg2) {
            const ret = arg0.set(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_set_991082a7a49971cf: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(arg0, arg1, arg2);
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_set_behavior_58b1c2633b3b9627: function() { return logError(function (arg0, arg1) {
            arg0.behavior = __wbindgen_enum_ScrollBehavior[arg1];
        }, arguments); },
        __wbg_set_behavior_b5de8e2d8b81f47a: function() { return logError(function (arg0, arg1) {
            arg0.behavior = __wbindgen_enum_ScrollBehavior[arg1];
        }, arguments); },
        __wbg_set_block_f69d4e9d5025b86e: function() { return logError(function (arg0, arg1) {
            arg0.block = __wbindgen_enum_ScrollLogicalPosition[arg1];
        }, arguments); },
        __wbg_set_dropEffect_245377f2f8101e46: function() { return logError(function (arg0, arg1, arg2) {
            arg0.dropEffect = getStringFromWasm0(arg1, arg2);
        }, arguments); },
        __wbg_set_effectAllowed_5041b2fc75929bc0: function() { return logError(function (arg0, arg1, arg2) {
            arg0.effectAllowed = getStringFromWasm0(arg1, arg2);
        }, arguments); },
        __wbg_set_href_c8a400c2767018aa: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.href = getStringFromWasm0(arg1, arg2);
        }, arguments); },
        __wbg_set_inline_ac42a6915a7bebff: function() { return logError(function (arg0, arg1) {
            arg0.inline = __wbindgen_enum_ScrollLogicalPosition[arg1];
        }, arguments); },
        __wbg_set_left_4ffc2196ec757c49: function() { return logError(function (arg0, arg1) {
            arg0.left = arg1;
        }, arguments); },
        __wbg_set_onclose_f791ef701be808a0: function() { return logError(function (arg0, arg1) {
            arg0.onclose = arg1;
        }, arguments); },
        __wbg_set_onload_3923655c18f34dac: function() { return logError(function (arg0, arg1) {
            arg0.onload = arg1;
        }, arguments); },
        __wbg_set_onmessage_d2fe701a9ce80846: function() { return logError(function (arg0, arg1) {
            arg0.onmessage = arg1;
        }, arguments); },
        __wbg_set_onopen_0556381d0db30cbb: function() { return logError(function (arg0, arg1) {
            arg0.onopen = arg1;
        }, arguments); },
        __wbg_set_scrollRestoration_cc9135c2711b821a: function() { return handleError(function (arg0, arg1) {
            arg0.scrollRestoration = __wbindgen_enum_ScrollRestoration[arg1];
        }, arguments); },
        __wbg_set_textContent_eaa1563abaeeb959: function() { return logError(function (arg0, arg1, arg2) {
            arg0.textContent = arg1 === 0 ? undefined : getStringFromWasm0(arg1, arg2);
        }, arguments); },
        __wbg_set_top_4d15f944c4461c70: function() { return logError(function (arg0, arg1) {
            arg0.top = arg1;
        }, arguments); },
        __wbg_shiftKey_60ce7360c7072983: function() { return logError(function (arg0) {
            const ret = arg0.shiftKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_shiftKey_9c96abc398f1063f: function() { return logError(function (arg0) {
            const ret = arg0.shiftKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_shiftKey_f53c305e9069c8d4: function() { return logError(function (arg0) {
            const ret = arg0.shiftKey;
            _assertBoolean(ret);
            return ret;
        }, arguments); },
        __wbg_size_55183c00ed76c4e1: function() { return logError(function (arg0) {
            const ret = arg0.size;
            return ret;
        }, arguments); },
        __wbg_stack_2ccfba4228dba9b9: function() { return logError(function (arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_state_2bb480c88adcea79: function() { return handleError(function (arg0) {
            const ret = arg0.state;
            return ret;
        }, arguments); },
        __wbg_static_accessor_GLOBAL_8dfb7f5e26ebe523: function() { return logError(function () {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_static_accessor_GLOBAL_THIS_941154efc8395cdd: function() { return logError(function () {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_static_accessor_SELF_58dac9af822f561f: function() { return logError(function () {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_static_accessor_WINDOW_ee64f0b3d8354c0b: function() { return logError(function () {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_statusText_d47258d1f4a842f0: function() { return logError(function (arg0, arg1) {
            const ret = arg1.statusText;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_stringify_b67e2c8c60b93f69: function() { return handleError(function (arg0) {
            const ret = JSON.stringify(arg0);
            return ret;
        }, arguments); },
        __wbg_tangentialPressure_f7c7d970e99d800f: function() { return logError(function (arg0) {
            const ret = arg0.tangentialPressure;
            return ret;
        }, arguments); },
        __wbg_targetTouches_5e97026a15363edf: function() { return logError(function (arg0) {
            const ret = arg0.targetTouches;
            return ret;
        }, arguments); },
        __wbg_target_c2d80ee4d3287cbb: function() { return logError(function (arg0) {
            const ret = arg0.target;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_textContent_c1c48d6ac47606b1: function() { return logError(function (arg0, arg1) {
            const ret = arg1.textContent;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_then_0150352e4ad20344: function() { return logError(function (arg0, arg1, arg2) {
            const ret = arg0.then(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_then_5160486c67ddb98a: function() { return logError(function (arg0, arg1) {
            const ret = arg0.then(arg1);
            return ret;
        }, arguments); },
        __wbg_tiltX_72c4437d1c89ad8d: function() { return logError(function (arg0) {
            const ret = arg0.tiltX;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_tiltY_745539b7730367bc: function() { return logError(function (arg0) {
            const ret = arg0.tiltY;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_time_faccfe41e2c1b9a2: function() { return logError(function (arg0) {
            const ret = arg0.time;
            return ret;
        }, arguments); },
        __wbg_top_7190df445227226f: function() { return logError(function (arg0) {
            const ret = arg0.top;
            return ret;
        }, arguments); },
        __wbg_touches_2617b6114abee3ce: function() { return logError(function (arg0) {
            const ret = arg0.touches;
            return ret;
        }, arguments); },
        __wbg_twist_5ff9ec892eccc8f8: function() { return logError(function (arg0) {
            const ret = arg0.twist;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_type_25f14fe7f9c6d777: function() { return logError(function (arg0, arg1) {
            const ret = arg1.type;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_type_9e0cd0d8df6e155a: function() { return logError(function (arg0, arg1) {
            const ret = arg1.type;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_type_a52042b72e3972c2: function() { return logError(function (arg0, arg1) {
            const ret = arg1.type;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_update_memory_9aa9738a1d9bdd7a: function() { return logError(function (arg0, arg1) {
            arg0.update_memory(arg1);
        }, arguments); },
        __wbg_value_49c7d3eb375cb662: function() { return logError(function (arg0, arg1) {
            const ret = arg1.value;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_value_5f8653fa438e0c94: function() { return logError(function (arg0, arg1) {
            const ret = arg1.value;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_value_c3b0708d699ea640: function() { return logError(function (arg0, arg1) {
            const ret = arg1.value;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_value_d5b248ce8419bd1b: function() { return logError(function (arg0) {
            const ret = arg0.value;
            return ret;
        }, arguments); },
        __wbg_weak_6452be576d2f48a0: function() { return logError(function (arg0) {
            const ret = arg0.weak();
            return ret;
        }, arguments); },
        __wbg_width_424e80d295f0c106: function() { return logError(function (arg0) {
            const ret = arg0.width;
            return ret;
        }, arguments); },
        __wbg_width_636a83de4d28badb: function() { return logError(function (arg0) {
            const ret = arg0.width;
            return ret;
        }, arguments); },
        __wbg_width_bd8275635c700ef4: function() { return logError(function (arg0) {
            const ret = arg0.width;
            _assertNum(ret);
            return ret;
        }, arguments); },
        __wbg_x_3f75df9d2e78b04d: function() { return logError(function (arg0) {
            const ret = arg0.x;
            return ret;
        }, arguments); },
        __wbg_y_631fea1d10bd951f: function() { return logError(function (arg0) {
            const ret = arg0.y;
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000001: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 1069, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, _ZN12wasm_bindgen7convert8closures1_6invoke17hd4590f821fc989feE);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000002: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 56, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, _ZN12wasm_bindgen7convert8closures1_6invoke17hb92d0d2677eacd7dE);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000003: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 275, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, _ZN12wasm_bindgen7convert8closures1_6invoke17h0f35b5db0f24afa3E);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000004: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("Event")], shim_idx: 276, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, _ZN12wasm_bindgen7convert8closures1_6invoke17h313e30b2f7725e31E);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000005: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 278, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, _ZN12wasm_bindgen7convert8closures1_6invoke17h5c29a2c7942d24c1E);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000006: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Ref(NamedExternref("Event"))], shim_idx: 277, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
            const ret = makeClosure(arg0, arg1, _ZN12wasm_bindgen7convert8closures1_1_6invoke17had9cfbc88a5da781E);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000007: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 620, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, _ZN12wasm_bindgen7convert8closures1_6invoke17h505e795a44f34719E);
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000008: function() { return logError(function (arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        }, arguments); },
        __wbindgen_cast_0000000000000009: function() { return logError(function (arg0) {
            // Cast intrinsic for `I64 -> Externref`.
            const ret = arg0;
            return ret;
        }, arguments); },
        __wbindgen_cast_000000000000000a: function() { return logError(function (arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        }, arguments); },
        __wbindgen_cast_000000000000000b: function() { return logError(function (arg0) {
            // Cast intrinsic for `U64 -> Externref`.
            const ret = BigInt.asUintN(64, arg0);
            return ret;
        }, arguments); },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./vmux_terminal_app_bg.js": import0,
        "./snippets/dioxus-interpreter-js-cee195cd562b7a49/src/js/patch_console.js": import1,
    };
}


//#endregion
function _ZN12wasm_bindgen7convert8closures1_6invoke17h505e795a44f34719E(arg0, arg1) {
    _assertNum(arg0);
    _assertNum(arg1);
    wasm._ZN12wasm_bindgen7convert8closures1_6invoke17h505e795a44f34719E(arg0, arg1);
}

function _ZN12wasm_bindgen7convert8closures1_6invoke17h8176746aa406cec1E(arg0, arg1) {
    _assertNum(arg0);
    _assertNum(arg1);
    const ret = wasm._ZN12wasm_bindgen7convert8closures1_6invoke17h8176746aa406cec1E(arg0, arg1);
    return ret !== 0;
}

function _ZN12wasm_bindgen7convert8closures1_6invoke17hb92d0d2677eacd7dE(arg0, arg1, arg2) {
    _assertNum(arg0);
    _assertNum(arg1);
    wasm._ZN12wasm_bindgen7convert8closures1_6invoke17hb92d0d2677eacd7dE(arg0, arg1, arg2);
}

function _ZN12wasm_bindgen7convert8closures1_6invoke17h0f35b5db0f24afa3E(arg0, arg1, arg2) {
    _assertNum(arg0);
    _assertNum(arg1);
    wasm._ZN12wasm_bindgen7convert8closures1_6invoke17h0f35b5db0f24afa3E(arg0, arg1, arg2);
}

function _ZN12wasm_bindgen7convert8closures1_6invoke17h313e30b2f7725e31E(arg0, arg1, arg2) {
    _assertNum(arg0);
    _assertNum(arg1);
    wasm._ZN12wasm_bindgen7convert8closures1_6invoke17h313e30b2f7725e31E(arg0, arg1, arg2);
}

function _ZN12wasm_bindgen7convert8closures1_6invoke17h5c29a2c7942d24c1E(arg0, arg1, arg2) {
    _assertNum(arg0);
    _assertNum(arg1);
    wasm._ZN12wasm_bindgen7convert8closures1_6invoke17h5c29a2c7942d24c1E(arg0, arg1, arg2);
}

function _ZN12wasm_bindgen7convert8closures1_1_6invoke17had9cfbc88a5da781E(arg0, arg1, arg2) {
    _assertNum(arg0);
    _assertNum(arg1);
    wasm._ZN12wasm_bindgen7convert8closures1_1_6invoke17had9cfbc88a5da781E(arg0, arg1, arg2);
}

function _ZN12wasm_bindgen7convert8closures1_6invoke17hd4590f821fc989feE(arg0, arg1, arg2) {
    _assertNum(arg0);
    _assertNum(arg1);
    const ret = wasm._ZN12wasm_bindgen7convert8closures1_6invoke17hd4590f821fc989feE(arg0, arg1, arg2);
    if (ret[1]) {
        throw takeFromExternrefTable0(ret[0]);
    }
}


const __wbindgen_enum_ScrollBehavior = ["auto", "instant", "smooth"];


const __wbindgen_enum_ScrollLogicalPosition = ["start", "center", "end", "nearest"];


const __wbindgen_enum_ScrollRestoration = ["auto", "manual"];
const JSOwnerFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jsowner_free(ptr >>> 0, 1));


//#region intrinsics
function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function _assertBigInt(n) {
    if (typeof(n) !== 'bigint') throw new Error(`expected a bigint argument, found ${typeof(n)}`);
}

function _assertBoolean(n) {
    if (typeof(n) !== 'boolean') {
        throw new Error(`expected a boolean argument, found ${typeof(n)}`);
    }
}

function _assertNum(n) {
    if (typeof(n) !== 'number') throw new Error(`expected a number argument, found ${typeof(n)}`);
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => wasm.__wbindgen_destroy_closure(state.a, state.b));

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function logError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        let error = (function () {
            try {
                return e instanceof Error ? `${e.message}\n\nStack:\n${e.stack}` : e.toString();
            } catch(_) {
                return "<failed to stringify thrown value>";
            }
        }());
        console.error("wasm-bindgen: imported JS function that was not marked as `catch` threw an error:", error);
        throw e;
    }
}

function makeClosure(arg0, arg1, f) {
    const state = { a: arg0, b: arg1, cnt: 1 };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        try {
            return f(state.a, state.b, ...args);
        } finally {
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            wasm.__wbindgen_destroy_closure(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function makeMutClosure(arg0, arg1, f) {
    const state = { a: arg0, b: arg1, cnt: 1 };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            wasm.__wbindgen_destroy_closure(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (typeof(arg) !== 'string') throw new Error(`expected a string argument, found ${typeof(arg)}`);
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);
        if (ret.read !== arg.length) throw new Error('failed to pass whole string');
        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;


//#endregion

//#region wasm loading
let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('vmux_terminal_app_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
//#endregion
export { wasm as __wasm }

globalThis.__wasm_split_main_initSync = initSync;

// Actually perform the load
__wbg_init({module_or_path: "/./wasm/vmux_terminal_app_bg.wasm"}).then((wasm) => {
    // assign this module to be accessible globally
    globalThis.__dx_mainWasm = wasm;
    globalThis.__dx_mainInit = __wbg_init;
    globalThis.__dx_mainInitSync = initSync;
    globalThis.__dx___wbg_get_imports = __wbg_get_imports;

    if (wasm.__wbindgen_start == undefined) {
        wasm.main();
    }
});

