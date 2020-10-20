use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};

pub const MESSAGE_ATTR: &str = "rtype";

pub fn expand(ast: &syn::DeriveInput) -> TokenStream {
    let item_type = {
        match get_attribute_type_multiple(ast, MESSAGE_ATTR) {
            Ok(ty) => match ty.len() {
                1 => ty[0].clone(),
                _ => {
                    return syn::Error::new(
                        Span::call_site(),
                        format!(
                            "#[{}(type)] takes 1 parameters, given {}",
                            MESSAGE_ATTR,
                            ty.len()
                        ),
                    )
                    .to_compile_error()
                }
            },
            Err(err) => return err.to_compile_error(),
        }
    };

    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let item_type = item_type
        .map(ToTokens::into_token_stream)
        .unwrap_or_else(|| quote! { () });

    quote! {
        impl #impl_generics ::actix::Message for #name #ty_generics #where_clause {
            type Result = #item_type;
        }
    }
}

fn get_attribute_type_multiple(
    ast: &syn::DeriveInput,
    name: &str,
) -> syn::Result<Vec<Option<syn::Type>>> {
    let attr = ast
        .attrs
        .iter()
        .find_map(|a| {
            let a = a.parse_meta();
            match a {
                Ok(meta) => {
                    if meta.path().is_ident(name) {
                        Some(meta)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        })
        .ok_or_else(|| {
            syn::Error::new(Span::call_site(), format!("Expect a attribute `{}`", name))
        })?;

    if let syn::Meta::List(ref list) = attr {
        Ok(list
            .nested
            .iter()
            .map(|m| meta_item_to_ty(m).ok())
            .collect())
    } else {
        Err(syn::Error::new_spanned(
            attr,
            format!("The correct syntax is #[{}(type, type, ...)]", name),
        ))
    }
}

fn meta_item_to_ty(meta_item: &syn::NestedMeta) -> syn::Result<syn::Type> {
    match meta_item {
        syn::NestedMeta::Meta(syn::Meta::Path(ref path)) => match path.get_ident() {
            Some(ident) => syn::parse_str::<syn::Type>(&ident.to_string())
                .map_err(|_| syn::Error::new_spanned(ident, "Expect type")),
            None => Err(syn::Error::new_spanned(path, "Expect type")),
        },
        syn::NestedMeta::Meta(syn::Meta::NameValue(val)) => match val.path.get_ident() {
            Some(ident) if ident == "result" => {
                if let syn::Lit::Str(ref s) = val.lit {
                    if let Ok(ty) = syn::parse_str::<syn::Type>(&s.value()) {
                        return Ok(ty);
                    }
                }
                Err(syn::Error::new_spanned(&val.lit, "Expect type"))
            }
            _ => Err(syn::Error::new_spanned(
                &val.lit,
                r#"Expect `result = "TYPE"`"#,
            )),
        },
        syn::NestedMeta::Lit(syn::Lit::Str(ref s)) => {
            syn::parse_str::<syn::Type>(&s.value())
                .map_err(|_| syn::Error::new_spanned(s, "Expect type"))
        }

        meta => Err(syn::Error::new_spanned(meta, "Expect type")),
    }
}
