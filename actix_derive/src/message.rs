use quote;
use syn;

pub const RESULT_TYPE: &str = "MessageResult";
pub const ERROR_TYPE: &str = "MessageError";
pub const COMBINED_TYPE: &str = "Message";

pub fn expand(ast: &syn::DeriveInput) -> quote::Tokens {
    if ::exists_attribute(ast, COMBINED_TYPE) && ::exists_attribute(ast, RESULT_TYPE) {
        panic!("Cannot use #[{}] and #[{}] on the same type", COMBINED_TYPE, RESULT_TYPE);
    }

    if ::exists_attribute(ast, COMBINED_TYPE) && ::exists_attribute(ast, ERROR_TYPE) {
        panic!("Cannot use #[{}] and #[{}] on the same type", COMBINED_TYPE, ERROR_TYPE);
    }

    let (item_type, error_type) = {
        if let Some(ty) = ::get_attribute_type_multiple(ast, COMBINED_TYPE) {
            match ty.len() {
                1 => (ty[0].clone(), None),
                2 => (ty[0].clone(), ty[1].clone()),
                _ => panic!("#[{}(type, type)] takes 2 parameters, given {}", COMBINED_TYPE, ty.len()),
            }
        } else {
            let item_type = ::get_attribute_type(ast, RESULT_TYPE);
            let error_type = ::get_attribute_type(ast, ERROR_TYPE);
            (item_type, error_type)
        }
    };

    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let item_type = item_type.unwrap_or(syn::Ty::Tup(vec![]));
    let error_type = error_type.unwrap_or(syn::Ty::Tup(vec![]));

    quote!{
        impl #impl_generics ResponseType for #name #ty_generics #where_clause {
            type Item = #item_type;
            type Error = #error_type;
        }
    }
}
