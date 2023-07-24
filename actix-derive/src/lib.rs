#![deny(rust_2018_idioms, nonstandard_style, future_incompatible)]
#![doc(html_logo_url = "https://actix.rs/img/logo.png")]
#![doc(html_favicon_url = "https://actix.rs/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![recursion_limit = "128"]

use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;

mod message;
mod message_response;

#[proc_macro_derive(Message, attributes(rtype))]
pub fn message_derive_rtype(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();

    message::expand(&ast).into()
}

#[proc_macro_derive(MessageResponse)]
pub fn message_response_derive_rtype(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();

    message_response::expand(&ast).into()
}

/// Marks async main function as the `actix` system entry-point.
///
/// # Examples
///
/// ```ignore
/// #[actix::main]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
#[proc_macro_attribute]
pub fn main(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut output: TokenStream = (quote! {
        #[::actix::__private::main(system = "::actix::System")]
    })
    .into();

    output.extend(item);
    output
}

/// Marks async test functions to use the `actix` system entry-point.
///
/// # Examples
///
/// ```ignore
/// #[actix::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut output: TokenStream = (quote! {
        #[::actix::__private::test(system = "::actix::System")]
    })
    .into();

    output.extend(item);
    output
}
