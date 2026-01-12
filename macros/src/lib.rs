//! Proc macros for ESP32-compatible testing.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Mark a function as a test that works on both host and ESP32.
///
/// This macro:
/// 1. Adds `#[test]` so the compiler collects it
/// 2. Calls a shared ESP-IDF initializer on ESP32 targets
///
/// # Example
///
/// ```ignore
/// use reticulum_rs_esp32_macros::esp32_test;
///
/// #[esp32_test]
/// fn my_test() {
///     assert_eq!(2 + 2, 4);
/// }
///
/// #[esp32_test]
/// #[should_panic(expected = "error message")]
/// fn my_panic_test() {
///     panic!("error message");
/// }
/// ```
#[proc_macro_attribute]
pub fn esp32_test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_block = &input_fn.block;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs; // Preserves #[should_panic] etc.
    let fn_sig = &input_fn.sig;

    let expanded = quote! {
        #[test]
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            // Initialize ESP-IDF once (shared across all tests)
            #[cfg(feature = "esp32")]
            {
                crate::ensure_esp_initialized();
            }

            #fn_block
        }
    };

    TokenStream::from(expanded)
}
