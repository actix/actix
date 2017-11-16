#![cfg_attr(feature = "nightly", feature(proc_macro))]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;

mod actor;
mod message;

#[cfg(feature = "nightly")]
macro_rules! create_attribute(
    ($mod_: ident, $fn_name: ident) => {
        #[proc_macro_attribute]
        pub fn $fn_name(attribute: TokenStream, input: TokenStream) -> TokenStream {
            let attribute = attribute.to_string();
            let input = input.to_string();
            let attribute = {
                if attribute.len() != 0 {
                    syn::parse_expr(&attribute).unwrap()
                } else {
                    syn::parse_expr("()").unwrap()
                }
            };
            let input = syn::parse_item(&input).unwrap();
            $mod_::$fn_name(&attribute, &input).parse().expect("Expanded output was no correct Rust code")
        }
    };
);

macro_rules! create_derive(
    ($mod_: ident, $trait_: ident, $fn_name: ident) => {
        #[proc_macro_derive($trait_)]
        #[doc(hidden)]
        pub fn $fn_name(input: TokenStream) -> TokenStream {
            let s = input.to_string();
            let ast = syn::parse_derive_input(&s).unwrap();
            $mod_::expand(&ast).parse().expect("Expanded output was no correct Rust code")
        }
    };
    ($mod_: ident, $trait_: ident, $fn_name: ident, $($attr: ident),+) => {
        #[proc_macro_derive($trait_, attributes($($attr),+))]
        #[doc(hidden)]
        pub fn $fn_name(input: TokenStream) -> TokenStream {
            let s = input.to_string();
            let ast = syn::parse_derive_input(&s).unwrap();
            $mod_::expand(&ast).parse().expect("Expanded output was no correct Rust code")
        }
    };
);

#[cfg(feature = "handler")]
create_attribute!(actor, handler);

create_derive!(actor, Actor, actor_derive);
create_derive!(message, Message, message_derive, MessageResult, MessageError, Message);

fn exists_attribute(ast: &syn::DeriveInput, name: &str) -> bool {
    ast.attrs.iter().find(|a| a.name() == name).is_some()
}

fn get_attribute_type_multiple(ast: &syn::DeriveInput, name: &str) -> Option<Vec<Option<syn::Ty>>> {
    let attr = ast.attrs.iter().find(|a| a.name() == name);

    if attr.is_none() {
        return None;
    }

    let attr = attr.unwrap();

    if let syn::MetaItem::List(_, ref vec) = attr.value {
        Some(vec.iter().map(|m| meta_item_to_ty(m, name)).collect())
    } else {
        panic!("The correct syntax is #[{}(type, type, ...)]", name);
    }
}

fn get_attribute_type(ast: &syn::DeriveInput, name: &str) -> Option<syn::Ty> {
    let attr = ast.attrs.iter().find(|a| a.name() == name);

    if attr.is_none() {
        return None;
    }

    let attr = attr.unwrap();

    if let syn::MetaItem::List(_, ref vec) = attr.value {
        if vec.len() != 1 {
            panic!("#[{}(type)] takes 1 parameter, given {}", name, vec.len());
        }

        meta_item_to_ty(&vec[0], name)
    } else {
        panic!("The correct syntax is #[{}(type)]", name);
    }
}

fn meta_item_to_ty(meta_item: &syn::NestedMetaItem, name: &str) -> Option<syn::Ty> {
    if let syn::NestedMetaItem::MetaItem(syn::MetaItem::Word(ref i)) = *meta_item {
        let ty = syn::parse::ty(i.as_ref());
        match ty {
            syn::parse::IResult::Done(_, ty) => Some(ty),
            _ => None,
        }
    } else {
        panic!("The correct syntax is #[{}(type)]", name);
    }
}

#[cfg(feature = "nightly")]
fn remove_attr(input: &syn::Item, attr_name: &str) -> syn::Item {
    let mut input = input.clone();

    {
        let impl_items = match input.node {
            syn::ItemKind::Impl(_, _, _, _, _, ref mut impl_items) => impl_items,
            _ => panic!(),
        };

        for item in impl_items.iter_mut() {
            item.attrs = item.attrs.clone().into_iter()
                .filter(|a| a.value.name() != attr_name)
                .collect::<Vec<_>>();
        }
    }

    input
}