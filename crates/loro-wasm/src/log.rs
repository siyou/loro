use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use tracing_wasm::WASMLayerConfig;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
}

#[macro_export]
macro_rules! console_log {
    // Note that this is using the `log` function imported above during
    // `bare_bones`
    ($($t:tt)*) => ($crate::log::log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen(js_name = enableDebug)]
pub fn enable_debug() {
    tracing_wasm::set_as_global_default_with_config(WASMLayerConfig::default())
}