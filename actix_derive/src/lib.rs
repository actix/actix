extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;

mod actor;
mod message;

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

create_derive!(actor, Actor, actor_derive);
create_derive!(message, Message, message_derive, MessageResult, MessageError);

fn get_attribute_type(ast: &syn::DeriveInput, name: &str) -> Option<syn::Ty> {
    let attr = ast.attrs.iter().find(|a| a.name() == name);

    if attr.is_none() {
        return None;
    }

    let attr = attr.unwrap();

    if let syn::MetaItem::NameValue(_, ref a) = attr.value {
        if let syn::Lit::Str(ref value, _) = *a {
            let ty = syn::parse::ty(value);
            match ty {
                syn::parse::IResult::Done(_, ty) => return Some(ty),
                _ => panic!(""),
            }
        }
    }

    None
}