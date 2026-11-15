//! Borrowed NUL-terminated strings, as the interface passes them in both directions.
//!
//! Each holds a pointer to the string's first byte, never null, so a value is a valid
//! string by construction: a Rust caller builds one only from a `CStr`, and the library
//! hands one out only over a string it owns. Their C typedefs are written in
//! `cbindgen.toml`, because a generated one would be the pointer without its `const`.

use std::{
    ffi::{CStr, c_char},
    marker::PhantomData,
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr::NonNull,
};

/// NUL-terminated UTF-8 text, borrowed. Required unless a description says "or NULL".
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Text<'a>(NonNull<c_char>, PhantomData<&'a CStr>);

/// NUL-terminated UTF-8 text, borrowed, or NULL.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct OptionalText<'a>(Option<NonNull<c_char>>, PhantomData<&'a CStr>);

/// A NUL-terminated path in the filesystem's own bytes, borrowed. A caller passes back
/// a path the library gave it unchanged, and never compares or re-encodes one.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PathBytes<'a>(NonNull<c_char>, PhantomData<&'a CStr>);

/// A NUL-terminated path in the filesystem's own bytes, borrowed, or NULL.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct OptionalPathBytes<'a>(Option<NonNull<c_char>>, PhantomData<&'a CStr>);

fn first(string: &CStr) -> NonNull<c_char> {
    NonNull::from(string).cast()
}

/// The string `first` begins, for as long as the value holding it borrows it.
fn whole<'a>(first: NonNull<c_char>) -> &'a CStr {
    // SAFETY: every value of these types was built from a `CStr` that outlives `'a`, or
    // handed over by a C caller under the header's contract that it is a NUL-terminated
    // string valid for the call.
    unsafe { CStr::from_ptr(first.as_ptr()) }
}

impl<'a> Text<'a> {
    pub fn new(text: &'a CStr) -> Self {
        Self(first(text), PhantomData)
    }

    pub fn as_c_str(self) -> &'a CStr {
        whole(self.0)
    }

    /// The text, or `None` when it is not UTF-8.
    pub fn to_str(self) -> Option<&'a str> {
        self.as_c_str().to_str().ok()
    }
}

impl<'a> From<&'a CStr> for Text<'a> {
    fn from(text: &'a CStr) -> Self {
        Self::new(text)
    }
}

impl<'a> OptionalText<'a> {
    pub const fn null() -> Self {
        Self(None, PhantomData)
    }

    pub fn new(text: &'a CStr) -> Self {
        Self(Some(first(text)), PhantomData)
    }

    pub fn as_c_str(self) -> Option<&'a CStr> {
        self.0.map(whole)
    }
}

/// The `count` texts a C caller passed as an array beginning at `first`, each as UTF-8;
/// `None` for a negative count, a NULL array of some, or a text that is not UTF-8.
pub(crate) fn texts<'a>(first: Option<NonNull<Text<'a>>>, count: i64) -> Option<Vec<&'a str>> {
    let count = usize::try_from(count).ok()?;
    if count == 0 {
        return Some(Vec::new());
    }
    // SAFETY: the header's contract is that a non-NULL `first` points at `count` texts
    // valid for the call.
    let texts = unsafe { std::slice::from_raw_parts(first?.as_ptr(), count) };
    texts.iter().map(|text| text.to_str()).collect()
}

impl<'a> From<&'a CStr> for OptionalText<'a> {
    fn from(text: &'a CStr) -> Self {
        Self::new(text)
    }
}

impl<'a> PathBytes<'a> {
    pub fn new(path: &'a CStr) -> Self {
        Self(first(path), PhantomData)
    }

    pub fn as_c_str(self) -> &'a CStr {
        whole(self.0)
    }

    pub fn as_path(self) -> &'a Path {
        Path::new(std::ffi::OsStr::from_bytes(self.as_c_str().to_bytes()))
    }
}

impl<'a> From<&'a CStr> for PathBytes<'a> {
    fn from(path: &'a CStr) -> Self {
        Self::new(path)
    }
}

impl<'a> OptionalPathBytes<'a> {
    pub const fn null() -> Self {
        Self(None, PhantomData)
    }

    pub fn new(path: &'a CStr) -> Self {
        Self(Some(first(path)), PhantomData)
    }

    pub fn as_path(self) -> Option<&'a Path> {
        self.0
            .map(|first| Path::new(std::ffi::OsStr::from_bytes(whole(first).to_bytes())))
    }
}

impl<'a> From<&'a CStr> for OptionalPathBytes<'a> {
    fn from(path: &'a CStr) -> Self {
        Self::new(path)
    }
}
