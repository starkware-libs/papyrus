use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{parse_macro_input, ItemTrait, LitStr, Meta, TraitItem};

#[proc_macro_attribute]
pub fn versioned_rpc(attr: TokenStream, input: TokenStream) -> TokenStream {
    let version = parse_macro_input!(attr as syn::LitStr);
    let item_trait = parse_macro_input!(input as ItemTrait);

    let trait_name = &item_trait.ident;
    let visibility = &item_trait.vis;

    // generate the new method signatures with the version prefix
    let versioned_methods = item_trait
        .items
        .iter()
        .map(|item| {
            if let TraitItem::Fn(method) = item {
                let new_method = syn::TraitItemFn {
                    attrs: method
                        .attrs
                        .iter()
                        .filter(|attr| !matches!(attr.meta, Meta::NameValue(_)))
                        .map(|attr| {
                            let mut new_attr = attr.clone();
                            if attr.path().is_ident("method") {
                                let _ = attr.parse_nested_meta(|meta| {
                                    if meta.path.is_ident("name") {
                                        let value = meta.value()?;
                                        let method_name: LitStr = value.parse()?;
                                        let new_meta_str = format!(
                                            "method(name = \"{}_{}\")",
                                            version.value(),
                                            method_name.value()
                                        );
                                        new_attr.meta = syn::parse_str::<Meta>(&new_meta_str)?;
                                    }
                                    Ok(())
                                });
                            }
                            new_attr
                        })
                        .collect::<Vec<_>>(),
                    sig: method.sig.clone(),
                    default: method.default.clone(),
                    semi_token: method.semi_token,
                };
                new_method.into()
            } else {
                item.clone()
            }
        })
        .collect::<Vec<TraitItem>>();

    // generate the versioned trait with the new method signatures
    let versioned_trait = syn::ItemTrait {
        attrs: vec![syn::parse_quote!(#[rpc(server, client, namespace = "starknet")])],
        vis: visibility.clone(),
        unsafety: None,
        auto_token: None,
        ident: syn::Ident::new(&format!("{}{}", trait_name, version.value()), trait_name.span()),
        colon_token: None,
        supertraits: item_trait.supertraits.clone(),
        brace_token: item_trait.brace_token,
        items: versioned_methods,
        restriction: item_trait.restriction.clone(),
        generics: item_trait.generics.clone(),
        trait_token: item_trait.trait_token,
    };

    versioned_trait.to_token_stream().into()
}
