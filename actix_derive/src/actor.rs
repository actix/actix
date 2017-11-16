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

#[cfg(feature = "handler")]
pub fn handler(_attribute: &syn::Expr, input: &syn::Item) -> quote::Tokens {
    let (for_type, impl_items) = match input.node {
        syn::ItemKind::Impl(_, _, _, _, ref for_type, ref impl_items) => (for_type, impl_items),
        _ => panic!("#[handler] is only valid on impl blocks"),
    };

    let mut impls = vec![];

    for item in impl_items.iter() {
        let name = &item.ident;

        let handle = item.attrs.iter()
            .filter(|a| a.value.name() == "handle")
            .collect::<Vec<_>>();

        if handle.len() == 0 {
            continue;
        } else if handle.len() > 1 {
            panic!("Can only have 1 #[handle(...)] attributes on {}, found {}", name.as_ref(), handle.len());
        }

        let ty = match handle[0].value {
            syn::MetaItem::List(_, ref ty) => ty,
            _ => panic!("The correct syntax is #[handle(type)]"),
        };

        if ty.len() != 1 {
            panic!("The correct syntax is #[handle(type)]");
        }

        let ty = ::meta_item_to_ty(&ty[0], "handle")
            .unwrap();

        match item.node {
            syn::ImplItemKind::Method(..) => (),
            _ => panic!("#[handle(type)] is only valid on methods"),
        };

        impls.push(quote!{
            impl Handler<#ty> for #for_type {
                fn handle(&mut self, message: #ty, context: &mut Context<Self>) -> Response<Self, #ty> {
                    self.#name(message, context)
                }
            }
        });
    }

    let input = ::remove_attr(input, "handle");

    quote!{
        #input

        #(#impls)*
    }
}
