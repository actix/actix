#![recursion_limit = "128"]

extern crate proc_macro;

use proc_macro::TokenStream;
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
