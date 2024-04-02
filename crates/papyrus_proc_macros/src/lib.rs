use std::str::FromStr;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, ExprLit, ItemFn, ItemTrait, LitBool, LitStr, Meta, TraitItem};

/// This macro is a wrapper around the "rpc" macro supplied by the jsonrpsee library that generates
/// a server and client traits from a given trait definition. The wrapper gets a version id and
/// prepend the version id to the trait name and to every method name (note method name refers to
/// the name the API has for the function not the actual function name). We need this in order to be
/// able to merge multiple versions of jsonrpc APIs into one server and not have a clash in method
/// resolution.
///
/// # Example:
///
/// Given this code:
/// ```rust,ignore
/// #[versioned_rpc("V0_6_0")]
/// pub trait JsonRpc {
///     #[method(name = "blockNumber")]
///     fn block_number(&self) -> Result<BlockNumber, Error>;
/// }
/// ```
///
/// The macro will generate this code:
/// ```rust,ignore
/// #[rpc(server, client, namespace = "starknet")]
/// pub trait JsonRpcV0_6_0 {
///     #[method(name = "V0_6_0_blockNumber")]
///     fn block_number(&self) -> Result<BlockNumber, Error>;
/// }
/// ```
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

/// This macro will emit a histogram metric with the given name and the latency of the function.
/// The macro also receives a boolean that controls if the metric should be controlled
/// by the profiling configuration value or not.
///
/// # Example
/// Given this code:
///
/// ```rust,ignore
/// #[latency_histogram("metric_name", false)]
/// fn foo() {
///     // Some code ...
/// }
/// ```
/// Every call to foo will update the histogram metric with the name “metric_name” with the time it
/// took to execute foo.
/// The metric will be emitted regardless of the value of the profiling configuration,
/// since the config value is false.
#[proc_macro_attribute]
pub fn latency_histogram(attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut input_fn = parse_macro_input!(input as ItemFn);
    let parts = attr
        .to_string()
        .split(',')
        .map(|s| {
            TokenStream::from_str(s)
                .expect("attribute should include metric name and controll with config boolean")
        })
        .collect::<Vec<_>>();
    let metric_name_as_tokenstream = parts
        .first()
        .expect("attribute should include metric name and controll with config boolean")
        .clone();
    let controll_with_config_as_tokenstream = parts
        .get(1)
        .expect("attribute should include metric name and controll with config boolean")
        .clone();
    let metric_name = parse_macro_input!(metric_name_as_tokenstream as ExprLit);
    let controll_with_config = parse_macro_input!(controll_with_config_as_tokenstream as LitBool);
    let origin_block = &mut input_fn.block;

    // Create a new block with the metric update.
    let expanded_block = quote! {
        {
            let mut start_function_time = None;
            if !#controll_with_config || (#controll_with_config && *(papyrus_common::metrics::COLLECT_PROFILING_METRICS.get().unwrap_or(&false))) {
                start_function_time=Some(std::time::Instant::now());
            }
            let return_value=#origin_block;
            if let Some(start_time) = start_function_time {
                metrics::histogram!(#metric_name, start_time.elapsed().as_secs_f64());
            }
            return_value
        }
    };

    // Create a new function with the modified block.
    let modified_function = ItemFn {
        block: syn::parse2(expanded_block).expect("Parse tokens in latency_histogram attribute."),
        ..input_fn
    };

    modified_function.to_token_stream().into()
}
