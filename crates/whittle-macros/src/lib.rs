//! Whittle proc-macros.
//!
//! This crate provides [`pattern!`], a function-like macro that turns a
//! string literal into a [`Pattern<RE>`] rule type while validating the
//! regular expression **at compile time**. A typo in the pattern is a
//! build error rather than a runtime panic on first construction.
//!
//! The macro is re-exported from `whittle-core` (and the `whittle`
//! facade) behind the `regex` feature; consumers write
//! `whittle::pattern!(r"...")` rather than depending on this crate
//! directly.
//!
//! [`Pattern<RE>`]: ../whittle_core/primitive/pattern/struct.Pattern.html

use proc_macro::TokenStream;

use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{LitStr, parse_macro_input};

/// Expand a string literal into a compile-time-validated
/// [`Pattern<RE>`] rule type.
///
/// `pattern!(r"...")` parses its argument as a string literal,
/// validates that it is a well-formed regular expression, and expands
/// to the const-generic rule type `Pattern::<"...">`. A malformed
/// pattern is reported as a compile error pointing at the literal.
///
/// Use it anywhere a `Pattern` type is expected, for example:
///
/// ```
/// # #![feature(adt_const_params, unsized_const_params)]
/// # #![expect(incomplete_features, reason = "doctest stand-in for whittle Pattern")]
/// # extern crate self as whittle;
/// # pub mod primitive {
/// #     pub struct Pattern<const RE: &'static str>;
/// # }
/// use whittle_macros::pattern;
///
/// type Name = pattern!(r"^(?:[A-Z])(?:-?[A-Za-z]+)*$");
///
/// fn main() {
///     let _type_name = core::any::type_name::<Name>();
/// }
/// ```
///
/// [`Pattern<RE>`]: ../whittle_core/primitive/pattern/struct.Pattern.html
#[proc_macro]
pub fn pattern(input: TokenStream) -> TokenStream {
    let literal = parse_macro_input!(input as LitStr);

    // Compile-time validation: a malformed pattern becomes a build
    // error at the literal's span instead of a runtime panic on first
    // `Pattern<RE>` construction.
    if let Err(error) = regex::Regex::new(&literal.value()) {
        return syn::Error::new(literal.span(), format!("invalid regex: {error}"))
            .to_compile_error()
            .into();
    }

    let crate_path = whittle_path();
    quote! { #crate_path::primitive::Pattern::<#literal> }.into()
}

/// Resolve the path to the crate that re-exports `Pattern`.
///
/// Consumers usually depend on the `whittle` facade, so the macro
/// expands to `::whittle::primitive::Pattern`. When a crate depends on
/// `whittle-core` directly (and renames or omits the facade),
/// `crate_name` reports `whittle-core` instead.
fn whittle_path() -> proc_macro2::TokenStream {
    // Prefer the `whittle` facade; fall back to `whittle-core` when the
    // consumer depends on the kernel directly.
    if let Ok(found) = crate_name("whittle") {
        return found_crate_path(&found, "whittle");
    }
    if let Ok(found) = crate_name("whittle-core") {
        return found_crate_path(&found, "whittle_core");
    }
    // Neither name resolved (e.g. an unusual rename). Fall back to the
    // facade name; a wrong guess surfaces as a clear unresolved-path
    // error at the macro call site.
    quote! { ::whittle }
}

/// Convert a [`FoundCrate`] into the leading path tokens for the
/// re-exporting crate.
///
/// `fallback` is the snake-case crate name that produced this
/// `FoundCrate` (`whittle` or `whittle_core`); it is used for the
/// `Itself` case. `FoundCrate::Itself` arises both when the defining
/// crate uses the macro internally and — importantly — when *its own*
/// doctests run (rustdoc compiles each doctest as a separate crate that
/// depends on the crate-under-test by its real name). A bare `crate::`
/// path is wrong in the doctest case, so emit the crate's own name as an
/// absolute path (`::whittle` / `::whittle_core`): it resolves in
/// downstream crates, in the defining crate (the crate name is in the
/// extern prelude), and in doctests alike.
fn found_crate_path(found: &FoundCrate, fallback: &str) -> proc_macro2::TokenStream {
    let name = match found {
        FoundCrate::Itself => fallback,
        FoundCrate::Name(renamed) => renamed.as_str(),
    };
    let ident = proc_macro2::Ident::new(name, proc_macro2::Span::call_site());
    quote! { ::#ident }
}
