use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::parse::Parser as _;

type AttributeArgs = syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>;

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
    let mut target_attrs = ast.attrs.iter().filter(|attr| attr.path().is_ident(name));

    if let Some(Ok(typ)) = target_attrs
        .clone()
        .find_map(|attr| attr.parse_args::<syn::LitStr>().ok())
        .map(|lit_str| lit_str.parse::<syn::Type>())
    {
        return Ok(vec![Some(typ)]);
    }

    let attr = target_attrs
        .find_map(|attr| attr.parse_args().ok())
        .ok_or_else(|| {
            syn::Error::new(Span::call_site(), format!("Expected an attribute `{name}`"))
        })?;

    match attr {
        syn::Meta::List(ref list) => {
            let parser = AttributeArgs::parse_terminated;
            let args = match parser.parse2(list.tokens.clone()) {
                Ok(args) => args,
                Err(_) => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        format!("The correct syntax is #[{name}(type, type, ...)]"),
                    ))
                }
            };

            Ok(args.iter().map(|m| meta_item_to_ty(m).ok()).collect())
        }

        syn::Meta::NameValue(ref nv) => match nv.path.get_ident() {
            Some(ident) if ident == "result" => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit),
                    ..
                }) = nv.value.clone()
                {
                    if let Ok(ty) = syn::parse_str::<syn::Type>(&lit.value()) {
                        return Ok(vec![Some(ty)]);
                    }
                }

                if let syn::Expr::Path(path) = nv.value.clone() {
                    if let Ok(ty) = syn::parse2::<syn::Type>(path.into_token_stream()) {
                        return Ok(vec![Some(ty)]);
                    }
                }

                Err(syn::Error::new_spanned(&nv.value, "expected type"))
            }
            _ => Err(syn::Error::new_spanned(
                &nv.value,
                r#"expected `result = TYPE`"#,
            )),
        },

        syn::Meta::Path(path) => match path.get_ident() {
            Some(ident) => syn::parse_str::<syn::Type>(&ident.to_string())
                .map(|ty| vec![Some(ty)])
                .map_err(|_| syn::Error::new_spanned(ident, "expected type")),
            None => Err(syn::Error::new_spanned(path, "expected type")),
        },
    }
}

fn meta_item_to_ty(meta_item: &syn::Meta) -> syn::Result<syn::Type> {
    match meta_item {
        syn::Meta::Path(path) => match path.get_ident() {
            Some(ident) => syn::parse_str::<syn::Type>(&ident.to_string())
                .map_err(|_| syn::Error::new_spanned(ident, "Expect type")),
            None => Err(syn::Error::new_spanned(path, "Expect type")),
        },
        syn::Meta::NameValue(nv) => match nv.path.get_ident() {
            Some(ident) if ident == "result" => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit),
                    ..
                }) = nv.value.clone()
                {
                    if let Ok(ty) = syn::parse_str::<syn::Type>(&lit.value()) {
                        return Ok(ty);
                    }
                }
                Err(syn::Error::new_spanned(&nv.value, "Expect type"))
            }
            _ => Err(syn::Error::new_spanned(
                &nv.value,
                r#"Expect `result = "TYPE"`"#,
            )),
        },
        syn::Meta::List(list) => {
            let lit_str = syn::parse2::<syn::LitStr>(list.tokens.clone())
                .map_err(|_| syn::Error::new_spanned(list, "Expect type"))?;

            syn::parse_str(&lit_str.value())
                .map_err(|_| syn::Error::new_spanned(list, "Expect type"))
        }
    }
}
