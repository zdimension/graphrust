use std::cmp::Ordering;
use std::ffi::CStr;

/// # Safety
///
/// The input pointer must point to a null-terminated UTF-8 string.
pub unsafe fn str_from_null_terminated_utf8<'a>(s: *const u8) -> &'a str {
    std::str::from_utf8_unchecked(CStr::from_ptr(s as *const _).to_bytes())
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
