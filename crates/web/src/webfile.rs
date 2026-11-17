//! Career files in the browser: an export is downloaded, an import is
//! chosen with the browser's own file picker.

use std::cell::RefCell;

use eframe::wasm_bindgen::prelude::*;
use eframe::web_sys::{self, js_sys};

thread_local! {
    /// Files the player chose, waiting for the game to take them.
    static CHOSEN: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
}

/// Hand `bytes` to the browser as a download called `name`.
pub(crate) fn download(name: &str, bytes: &[u8]) {
    let run = || -> Option<()> {
        let part = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&part);
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type("application/octet-stream");
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).ok()?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
        let doc = web_sys::window()?.document()?;
        let a: web_sys::HtmlAnchorElement = doc.create_element("a").ok()?.dyn_into().ok()?;
        a.set_href(&url);
        a.set_download(name);
        a.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Some(())
    };
    let _ = run();
}

/// Open the browser's file picker; the file arrives through `take`.
pub(crate) fn pick() {
    let run = || -> Option<()> {
        let doc = web_sys::window()?.document()?;
        let input: web_sys::HtmlInputElement = doc.create_element("input").ok()?.dyn_into().ok()?;
        input.set_type("file");
        input.set_accept(".bmsave,.json,application/json");
        let input2 = input.clone();
        let on_change = Closure::<dyn FnMut()>::new(move || {
            let Some(file) = input2.files().and_then(|f| f.get(0)) else { return };
            let Ok(reader) = web_sys::FileReader::new() else { return };
            let reader2 = reader.clone();
            let on_load = Closure::<dyn FnMut()>::new(move || {
                if let Ok(buf) = reader2.result() {
                    let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                    CHOSEN.with(|c| c.borrow_mut().push(bytes));
                }
            });
            reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
            on_load.forget();
            let _ = reader.read_as_array_buffer(&file);
        });
        input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
        on_change.forget();
        input.click();
        Some(())
    };
    let _ = run();
}

/// Files chosen since the last call.
pub(crate) fn take() -> Vec<Vec<u8>> {
    CHOSEN.with(|c| std::mem::take(&mut *c.borrow_mut()))
}
