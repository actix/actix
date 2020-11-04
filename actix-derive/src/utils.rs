use syn::Meta;

/// Finds an attribute matching an identifier
pub fn find_attribute_meta(ast: &syn::DeriveInput, attribute: &str) -> Option<Meta> {
    ast.attrs.iter().find_map(|attr| {
        let maybe_meta = attr.parse_meta().ok();
        maybe_meta.and_then(|meta| {
            if meta.path().is_ident(attribute) {
                Some(meta)
            } else {
                None
            }
        })
    })
}
