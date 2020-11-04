use crate::utils::find_attribute_meta;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::Meta;

pub const ACTOR_ATTR: &str = "actor";

pub fn expand(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;

    let context_type = find_attribute_meta(ast, ACTOR_ATTR)
        .and_then(get_context_type)
        .map(|t| t.into_token_stream())
        .unwrap_or(quote! { ::actix::dev::Context });

    quote! {
        impl ::actix::Actor for #name {
            type Context = #context_type<Self>;
        }
    }
}

fn get_context_type(meta: Meta) -> Option<syn::Type> {
    if let syn::Meta::List(ref list) = meta {
        if list.nested.is_empty() {
            return None;
        }

        get_context_type_from_meta(&list.nested[0])
    } else {
        None
    }
}

fn get_context_type_from_meta(item: &syn::NestedMeta) -> Option<syn::Type> {
    match item {
        syn::NestedMeta::Lit(syn::Lit::Str(ref s)) => {
            syn::parse_str::<syn::Type>(&s.value()).ok()
        }
        _ => None,
    }
}
