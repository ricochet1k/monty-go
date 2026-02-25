use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    ptr,
};

use monty::MontyException;
use thiserror::Error;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MontyStatus {
    pub ok: i32,
    pub error: *mut c_char,
}

impl MontyStatus {
    pub fn success() -> Self {
        Self {
            ok: 1,
            error: ptr::null_mut(),
        }
    }

    pub fn from_error(err: impl Into<FfiError>) -> Self {
        let err = err.into();
        let c_string = CString::new(err.to_string())
            .unwrap_or_else(|_| CString::new("monty-ffi error").unwrap());
        Self {
            ok: 0,
            error: c_string.into_raw(),
        }
    }
}

pub type FfiResult<T> = Result<T, FfiError>;

#[derive(Debug, Error)]
pub enum FfiError {
    #[error("{0}")]
    Message(String),
    #[error("null pointer for {0}")]
    NullPointer(&'static str),
    #[error("{field} is not valid UTF-8")]
    InvalidUtf8 { field: &'static str },
    #[error("string for {field} contains interior NUL bytes")]
    InteriorNul { field: &'static str },
}

impl From<MontyException> for FfiError {
    fn from(exc: MontyException) -> Self {
        Self::Message(exc.summary())
    }
}

impl From<serde_json::Error> for FfiError {
    fn from(err: serde_json::Error) -> Self {
        Self::Message(err.to_string())
    }
}

impl From<postcard::Error> for FfiError {
    fn from(err: postcard::Error) -> Self {
        Self::Message(err.to_string())
    }
}

impl From<std::str::Utf8Error> for FfiError {
    fn from(err: std::str::Utf8Error) -> Self {
        Self::Message(err.to_string())
    }
}

pub unsafe fn read_required_str(ptr: *const c_char, field: &'static str) -> FfiResult<String> {
    if ptr.is_null() {
        return Err(FfiError::NullPointer(field));
    }
    Ok(CStr::from_ptr(ptr)
        .to_str()
        .map_err(|_| FfiError::InvalidUtf8 { field })?
        .to_owned())
}

pub unsafe fn read_optional_str(ptr: *const c_char) -> FfiResult<Option<String>> {
    if ptr.is_null() {
        Ok(None)
    } else {
        Ok(Some(
            CStr::from_ptr(ptr)
                .to_str()
                .map_err(|_| FfiError::InvalidUtf8 { field: "string" })?
                .to_owned(),
        ))
    }
}

pub fn to_c_string(value: impl Into<String>, field: &'static str) -> FfiResult<*mut c_char> {
    let value = value.into();
    if value.bytes().any(|b| b == 0) {
        return Err(FfiError::InteriorNul { field });
    }
    Ok(CString::new(value).unwrap().into_raw())
}

#[no_mangle]
pub unsafe extern "C" fn monty_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
