//! The il2cpp C API exported by `GameAssembly.dll` (Unity 2020.3, metadata v27), looked up at run time.
//! All are `cdecl` on x86.

use std::ffi::{c_char, c_void, CStr, CString};

use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

pub type Ptr = *mut c_void;

macro_rules! api {
    ($($name:ident : fn($($a:ty),*) -> $r:ty;)*) => {
        /// Function pointers into `GameAssembly.dll`.
        #[allow(non_snake_case)]
        pub struct Api { $(pub $name: unsafe extern "C" fn($($a),*) -> $r,)* }

        impl Api {
            /// Look the API up in the loaded `GameAssembly.dll`; `None` until it is loaded.
            pub fn load() -> Option<Api> {
                unsafe {
                    let m = GetModuleHandleA(c"GameAssembly.dll".as_ptr() as *const u8);
                    if m.is_null() {
                        return None;
                    }
                    Some(Api { $($name: std::mem::transmute(GetProcAddress(m, concat!(stringify!($name), "\0").as_ptr())?),)* })
                }
            }
        }
    };
}

api! {
    il2cpp_domain_get: fn() -> Ptr;
    il2cpp_thread_attach: fn(Ptr) -> Ptr;
    il2cpp_domain_get_assemblies: fn(Ptr, *mut usize) -> *const Ptr;
    il2cpp_assembly_get_image: fn(Ptr) -> Ptr;
    il2cpp_image_get_name: fn(Ptr) -> *const c_char;
    il2cpp_class_from_name: fn(Ptr, *const c_char, *const c_char) -> Ptr;
    il2cpp_class_get_methods: fn(Ptr, *mut Ptr) -> Ptr;
    il2cpp_class_get_method_from_name: fn(Ptr, *const c_char, i32) -> Ptr;
    il2cpp_method_get_name: fn(Ptr) -> *const c_char;
    il2cpp_method_get_param_count: fn(Ptr) -> u32;
    il2cpp_method_get_param: fn(Ptr, u32) -> Ptr;
    il2cpp_method_get_return_type: fn(Ptr) -> Ptr;
    il2cpp_method_get_flags: fn(Ptr, *mut u32) -> u32;
    il2cpp_type_get_type: fn(Ptr) -> i32;
    il2cpp_type_get_name: fn(Ptr) -> *mut c_char;
    il2cpp_runtime_invoke: fn(Ptr, Ptr, *mut Ptr, *mut Ptr) -> Ptr;
    il2cpp_string_new: fn(*const c_char) -> Ptr;
    il2cpp_string_length: fn(Ptr) -> i32;
    il2cpp_string_chars: fn(Ptr) -> *const u16;
    il2cpp_object_unbox: fn(Ptr) -> Ptr;
    il2cpp_object_get_class: fn(Ptr) -> Ptr;
    il2cpp_class_get_name: fn(Ptr) -> *const c_char;
    il2cpp_array_new: fn(Ptr, usize) -> Ptr;
    il2cpp_array_length: fn(Ptr) -> u32;
    il2cpp_class_array_element_size: fn(Ptr) -> i32;
    il2cpp_free: fn(Ptr) -> ();
}

/// A C string from il2cpp (borrowed).
pub unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

pub fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

impl Api {
    /// The loaded assemblies' images, by name (`Assembly-CSharp.dll`, ...).
    pub unsafe fn images(&self, domain: Ptr) -> Vec<(String, Ptr)> {
        let mut n = 0usize;
        let list = (self.il2cpp_domain_get_assemblies)(domain, &mut n);
        (0..n)
            .map(|i| {
                let img = (self.il2cpp_assembly_get_image)(*list.add(i));
                (cstr((self.il2cpp_image_get_name)(img)), img)
            })
            .collect()
    }

    pub unsafe fn class(&self, image: Ptr, namespace: &str, name: &str) -> Ptr {
        (self.il2cpp_class_from_name)(image, c(namespace).as_ptr(), c(name).as_ptr())
    }

    /// Every method of a class: (name, parameter count, MethodInfo*).
    pub unsafe fn methods(&self, class: Ptr) -> Vec<(String, u32, Ptr)> {
        let mut iter: Ptr = std::ptr::null_mut();
        let mut out = Vec::new();
        loop {
            let m = (self.il2cpp_class_get_methods)(class, &mut iter);
            if m.is_null() {
                break;
            }
            out.push((cstr((self.il2cpp_method_get_name)(m)), (self.il2cpp_method_get_param_count)(m), m));
        }
        out
    }
}
