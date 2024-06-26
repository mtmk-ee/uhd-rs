#![allow(unused)]

use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::{addr_of, addr_of_mut},
};

use crate::{try_uhd, Result, UhdError};
pub use uhd_usrp_sys::*;

/// Wrapper for a heap-allocated UHD handle.
///
/// This struct wraps a pointer to a heap-allocated UHD handle,
/// a FFI function pointer to free it. The handle is freed when
/// this struct is dropped.
#[derive(Debug)]
pub(crate) struct OwnedHandle<T> {
    handle: *mut T,
    free: unsafe extern "C" fn(*mut *mut T) -> u32,
}

/// Helper struct for receiving strings via FFI.
///
/// This struct wraps a fixed-length array of bytes which can be used
/// to receive a string via FFI.
pub(crate) struct FfiString {
    s: Vec<u8>,
}

/// A vector of strings.
pub(crate) struct FfiStringVec {
    handle: OwnedHandle<uhd_usrp_sys::uhd_string_vector_t>,
}

impl<T> OwnedHandle<T> {
    /// Allocate a new handle using the given allocator function,
    /// and return it wrapped in an `OwnedHandle`.
    ///
    /// The handle is freed when it is dropped.
    pub fn new(
        alloc: unsafe extern "C" fn(*mut *mut T) -> u32,
        free: unsafe extern "C" fn(*mut *mut T) -> u32,
    ) -> Result<Self> {
        let mut handle = MaybeUninit::uninit();
        try_uhd!(unsafe { alloc(handle.as_mut_ptr()) })?;
        let handle = unsafe { handle.assume_init() };
        if handle.is_null() {
            Err(UhdError::Unknown)
        } else {
            Ok(Self { handle, free })
        }
    }

    /// Wrap a pointer to an existing handle.
    ///
    /// # Safety
    ///
    /// The object pointed to by the handle cannot be freed for the
    /// entire lifetime of the handle, and it must be safe to free
    /// it when it is dropped.
    ///
    /// The handle must be of a valid type `T`.
    ///
    /// # Panics
    ///
    /// Panics if the handle is null.
    pub unsafe fn from_ptr(handle: *mut T, free: unsafe extern "C" fn(*mut *mut T) -> u32) -> Self {
        if handle.is_null() {
            panic!("handle is null");
        }
        Self { handle, free }
    }

    /// Get a pointer to the handle.
    pub fn as_ptr(&self) -> *const T {
        self.handle
    }

    /// Get a mutable pointer to the handle.
    pub fn as_mut_ptr(&self) -> *mut T {
        self.handle
    }

    /// Get a mutable pointer to a mutable pointer to the handle.
    pub fn as_mut_mut_ptr(&self) -> *mut *mut T {
        addr_of!(self.handle).cast_mut()
    }
}

impl<T> Drop for OwnedHandle<T> {
    fn drop(&mut self) {
        unsafe {
            (self.free)(addr_of_mut!(self.handle));
        }
    }
}

impl<T> Deref for OwnedHandle<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.handle.as_ref().expect("handle is null") }
    }
}

impl<T> DerefMut for OwnedHandle<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.handle.as_mut().expect("handle is null") }
    }
}

impl FfiString {
    pub fn with_capacity(len: usize) -> Self {
        assert!(len != 0);
        Self { s: vec![0u8; len] }
    }

    pub fn as_mut_ptr(&mut self) -> *mut i8 {
        self.s.as_mut_ptr().cast()
    }

    /// The maximum number of characters that can be stored in this struct.
    ///
    /// The return value does not include the null terminator.
    pub fn max_chars(&self) -> usize {
        self.s.len()
    }

    /// Convert this struct into a string.
    ///
    /// Returns an error if the string is not valid UTF-8, or
    /// is not terminated by a null character.
    pub fn to_string(&self) -> Result<String> {
        Ok(CStr::from_bytes_until_nul(&self.s)
            .or(Err(UhdError::Unknown))?
            .to_string_lossy()
            .into_owned())
    }
}

impl FfiStringVec {
    /// Create a new empty string vector.
    pub fn new() -> FfiStringVec {
        Self {
            handle: OwnedHandle::new(
                uhd_usrp_sys::uhd_string_vector_make,
                uhd_usrp_sys::uhd_string_vector_free,
            )
            .unwrap(),
        }
    }

    pub fn as_ptr(&self) -> *const uhd_usrp_sys::uhd_string_vector_handle {
        self.handle.as_mut_mut_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut uhd_usrp_sys::uhd_string_vector_handle {
        self.handle.as_mut_mut_ptr()
    }

    /// Add a string to the vector.
    pub fn push(&mut self, value: &str) {
        let value = CString::new(value).unwrap();
        unsafe {
            uhd_usrp_sys::uhd_string_vector_push_back(self.handle.as_mut_mut_ptr(), value.as_ptr());
        }
    }

    /// Get the number of strings in the vector.
    pub fn len(&self) -> usize {
        let mut value = 0;
        unsafe {
            uhd_usrp_sys::uhd_string_vector_size(self.handle.as_mut_ptr(), addr_of_mut!(value))
        };
        value
    }

    /// Try to get the string at the given index.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn get(&self, index: usize) -> Option<String> {
        let mut s = FfiString::with_capacity(128);
        try_uhd!(unsafe {
            uhd_usrp_sys::uhd_string_vector_at(
                self.handle.as_mut_ptr(),
                index,
                s.as_mut_ptr(),
                s.max_chars(),
            )
        })
        .ok()?;
        s.to_string().ok()
    }

    /// Convert this type to a Rust [`Vec`].
    pub fn to_vec(&self) -> Vec<String> {
        let mut result = Vec::with_capacity(self.len());
        for i in 0..self.len() {
            result.push(self.get(i).unwrap());
        }
        result
    }
}
