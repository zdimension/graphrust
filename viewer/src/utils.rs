use std::cmp::Ordering;
use std::ffi::CStr;

/// Logs a message to the console prefixed with the current time and caller code location.
/*#[macro_export]
macro_rules! log
{
    ($($arg:tt)*) =>
    {
        //log::info!("[{}] [{}:{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), file!(), line!(), format_args!($($arg)*));
        log::info!($($arg)*);
        //$crate::utils::add_loading_text(&format!("{}", format_args!($($arg)*)));
    }
}
*/
pub fn add_loading_text(_text: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;

        let elem = eframe::web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("center_message")
            .unwrap();
        let elem = elem.dyn_ref::<eframe::web_sys::HtmlElement>().unwrap();

        let orig_text = elem.text_content().unwrap();
        let new_text = format!("{}\n{}", orig_text, text);

        elem.set_text_content(Some(&new_text));
    }
}

pub unsafe fn str_from_null_terminated_utf8<'a>(s: *const u8) -> &'a str {
    CStr::from_ptr(s as *const _).to_str().unwrap()
}

pub trait SliceExt {
    type Item;

    /// Creates mutable references to two items in a slice.
    fn get_two_mut(&mut self, index0: usize, index1: usize) -> (&mut Self::Item, &mut Self::Item);
}

impl<T> SliceExt for [T] {
    type Item = T;

    fn get_two_mut(&mut self, index0: usize, index1: usize) -> (&mut Self::Item, &mut Self::Item) {
        match index0.cmp(&index1) {
            Ordering::Less => {
                let mut iter = self.iter_mut();
                let item0 = iter.nth(index0).unwrap();
                let item1 = iter.nth(index1 - index0 - 1).unwrap();
                (item0, item1)
            }
            Ordering::Equal => panic!("[T]::get_two_mut(): received same index twice ({})", index0),
            Ordering::Greater => {
                let mut iter = self.iter_mut();
                let item1 = iter.nth(index1).unwrap();
                let item0 = iter.nth(index0 - index1 - 1).unwrap();
                (item0, item1)
            }
        }
    }
}
