use proc_macro::TokenStream;

use quote::quote;
use syn::{Data, DeriveInput, LitStr, parse_macro_input};

/// Derive `blackflower_ecs::Component` with a stable Flecs name.
#[proc_macro_derive(Component, attributes(ecs))]
pub fn derive_component(input: TokenStream) -> TokenStream {
    derive_ecs_trait(input, TraitKind::Component)
}

/// Derive `blackflower_ecs::Tag` for a fieldless marker type.
#[proc_macro_derive(Tag, attributes(ecs))]
pub fn derive_tag(input: TokenStream) -> TokenStream {
    derive_ecs_trait(input, TraitKind::Tag)
}

#[derive(Clone, Copy)]
enum TraitKind {
    Component,
    Tag,
}

fn derive_ecs_trait(input: TokenStream, trait_kind: TraitKind) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_ecs_trait(&input, trait_kind) {
        Ok(output) => output.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand_ecs_trait(
    input: &DeriveInput,
    trait_kind: TraitKind,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let identifier = &input.ident;
    if matches!(trait_kind, TraitKind::Tag) {
        validate_tag(input)?;
    }
    let name = component_name(input)?;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let trait_path = match trait_kind {
        TraitKind::Component => quote!(::blackflower_ecs::Component),
        TraitKind::Tag => quote!(::blackflower_ecs::Tag),
    };

    Ok(quote! {
        impl #impl_generics #trait_path for #identifier #type_generics #where_clause {
            const NAME: &'static str = #name;
        }
    })
}

fn component_name(input: &DeriveInput) -> Result<LitStr, syn::Error> {
    let mut configured_name = None;
    for attribute in &input.attrs {
        if !attribute.path().is_ident("ecs") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if !meta.path.is_ident("name") {
                return Err(meta.error("unsupported ecs attribute; expected `name = \"...\"`"));
            }
            if configured_name.is_some() {
                return Err(meta.error("duplicate ecs name"));
            }
            configured_name = Some(meta.value()?.parse::<LitStr>()?);
            Ok(())
        })?;
    }

    let name = configured_name
        .unwrap_or_else(|| LitStr::new(&input.ident.to_string(), input.ident.span()));
    if name.value().is_empty() || name.value().contains('\0') {
        return Err(syn::Error::new(
            name.span(),
            "ecs name must be non-empty and contain no NUL",
        ));
    }
    Ok(name)
}

fn validate_tag(input: &DeriveInput) -> Result<(), syn::Error> {
    match &input.data {
        Data::Struct(data) if data.fields.is_empty() => Ok(()),
        Data::Struct(_) | Data::Enum(_) | Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "Tag can only be derived for a fieldless struct",
        )),
    }
}
