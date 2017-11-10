use quote;
use syn;

pub const RESULT_TYPE: &str = "MessageResult";
pub const ERROR_TYPE: &str = "MessageError";

pub fn expand(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
	let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

	let item_type = ::get_attribute_type(ast, RESULT_TYPE)
		.unwrap_or(syn::Ty::Tup(vec![]));

	let error_type = ::get_attribute_type(ast, ERROR_TYPE)
		.unwrap_or(syn::Ty::Tup(vec![]));

    quote!{
        impl #impl_generics ResponseType for #name #ty_generics #where_clause {
            type Item = #item_type;
			type Error = #error_type;
        }
    }
}
