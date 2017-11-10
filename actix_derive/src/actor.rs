use quote;
use syn;

pub fn expand(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
	let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    quote!{
        impl #impl_generics Actor for #name #ty_generics #where_clause {
            type Context = Context<Self>;
        }
    }
}
