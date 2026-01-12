//! Procedural macros for TAP testing in reticulum-rs-esp32.
//!
//! This crate provides the `#[tap_test]` attribute macro for defining tests
//! that can run on host, QEMU, or real hardware using TAP output format.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Expr, ExprLit, ItemFn, Lit, Meta, ReturnType};

/// Mark a function as a TAP test.
///
/// The function will be registered with the test collector and run when
/// the test binary executes. Tests can either:
/// - Return nothing (panics indicate failure)
/// - Return `Result<(), E>` where `E: Error` (Err indicates failure)
///
/// # Attributes
///
/// - `#[tap_test]` - Regular test
/// - `#[tap_test(should_panic)]` - Test that should panic
/// - `#[tap_test(should_panic = "expected message")]` - Test that should panic with specific message
///
/// # Example
///
/// ```ignore
/// use reticulum_rs_esp32_macros::tap_test;
///
/// #[tap_test]
/// fn addition_works() {
///     assert_eq!(2 + 2, 4);
/// }
///
/// #[tap_test]
/// fn parsing_works() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
///     let value: i32 = "42".parse()?;
///     assert_eq!(value, 42);
///     Ok(())
/// }
///
/// #[tap_test(should_panic = "MTU must be greater")]
/// fn mtu_too_small_panics() {
///     Fragmenter::new(2);
/// }
/// ```
#[proc_macro_attribute]
pub fn tap_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let fn_block = &input_fn.block;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs;

    // Parse attributes for should_panic
    let should_panic = parse_should_panic(attr);

    // Check if function returns Result or ()
    let fn_output = &input_fn.sig.output;
    let returns_result = matches!(fn_output, ReturnType::Type(_, _));

    let test_fn = quote! {
        #(#fn_attrs)*
        #fn_vis fn #fn_name() #fn_output #fn_block
    };

    let register_call = match should_panic {
        ShouldPanic::No => {
            if returns_result {
                quote! {
                    runner.run(#fn_name_str, #fn_name);
                }
            } else {
                quote! {
                    runner.run_assert(#fn_name_str, #fn_name);
                }
            }
        }
        ShouldPanic::Yes => {
            quote! {
                runner.run_should_panic(#fn_name_str, #fn_name, None);
            }
        }
        ShouldPanic::WithMessage(msg) => {
            quote! {
                runner.run_should_panic(#fn_name_str, #fn_name, Some(#msg));
            }
        }
    };

    // Generate the test function and inventory registration
    let expanded = quote! {
        #test_fn

        ::inventory::submit! {
            ::reticulum_rs_esp32::testing::TapTestEntry::new(
                #fn_name_str,
                |runner: &mut ::reticulum_rs_esp32::testing::TestRunner| {
                    #register_call
                }
            )
        }
    };

    TokenStream::from(expanded)
}

enum ShouldPanic {
    No,
    Yes,
    WithMessage(String),
}

fn parse_should_panic(attr: TokenStream) -> ShouldPanic {
    if attr.is_empty() {
        return ShouldPanic::No;
    }

    let attr_str = attr.to_string();

    // Handle: should_panic
    if attr_str.trim() == "should_panic" {
        return ShouldPanic::Yes;
    }

    // Handle: should_panic = "message"
    if attr_str.starts_with("should_panic") {
        // Parse as meta
        let meta: Result<Meta, _> = syn::parse(attr);
        if let Ok(Meta::NameValue(nv)) = meta {
            if nv.path.is_ident("should_panic") {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) = nv.value
                {
                    return ShouldPanic::WithMessage(s.value());
                } else {
                    // Invalid value type (e.g., should_panic = 42)
                    panic!(
                        "tap_test: should_panic expects a string literal, \
                         e.g., #[tap_test(should_panic = \"expected message\")]"
                    );
                }
            }
        }
        // Fallback: bare should_panic parsed as path
        if let Ok(Meta::Path(p)) = syn::parse(attr_str.parse().unwrap()) {
            if p.is_ident("should_panic") {
                return ShouldPanic::Yes;
            }
        }
        // Unknown attribute format
        panic!(
            "tap_test: invalid should_panic syntax. Use #[tap_test(should_panic)] \
             or #[tap_test(should_panic = \"message\")]"
        );
    }

    // Unknown attribute
    panic!(
        "tap_test: unknown attribute '{}'. Supported: should_panic, should_panic = \"message\"",
        attr_str
    );
}
