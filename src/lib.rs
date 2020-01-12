/*! Derivation of [`Serialize`][serialize] and [`Deserialize`][deserialize] that replaces struct keys with numerical indices.

### Usage example
The macros currently understand `serde`'s [`skip_if_serialized`][skip-serializing-if] field attribute
and a custom `offset` container attribute.

```
use serde_indexed::{DeserializeIndexed, SerializeIndexed};

#[derive(Clone, Debug, PartialEq, SerializeIndexed, DeserializeIndexed)]
#[serde_indexed(offset = 1)]
pub struct SomeKeys {
    pub number: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<u8>,
    pub bytes: [u8; 7],
}
```

### Generated code example
`cargo expand --test basics` exercises the macros using [`serde_cbor`][serde-cbor].

[serialize]: https://docs.serde.rs/serde/ser/trait.Serialize.html
[deserialize]: https://docs.serde.rs/serde/de/trait.Deserialize.html
[skip-serializing-if]: https://serde.rs/field-attrs.html#skip_serializing_if
[serde-cbor]: https://docs.rs/serde_cbor
*/

extern crate proc_macro;

mod parse;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

use crate::parse::Input;

fn serialize_fields(fields: &[parse::Field], offset: usize) -> Vec<proc_macro2::TokenStream> {
    fields
        .iter()
        .map(|field| {
            let index = field.index + offset;
            let member = &field.member;
            // println!("field {:?} index {:?}", &field.label, field.index);
            match &field.skip_serializing_if {
                Some(path) => {
                    quote! {
                        if !#path(&self.#member) {
                            map.serialize_entry(&#index, &self.#member)?;
                        }
                    }
                }
                None => {
                    quote! {
                        map.serialize_entry(&#index, &self.#member)?;
                    }
                }
            }
        })
        .collect()
}

fn count_serialized_fields(fields: &[parse::Field]) -> Vec<proc_macro2::TokenStream> {
    fields
        .iter()
        .map(|field| {
            // let index = field.index + offset;
            let member = &field.member;
            match &field.skip_serializing_if {
                Some(path) => {
                    quote! { + if #path(&self.#member) { 0 } else { 1 } }
                }
                None => {
                    quote! { + 1 }
                }
            }
        })
        .collect()
}

#[proc_macro_derive(SerializeIndexed, attributes(serde, serde_indexed))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    let ident = input.ident;
    let num_fields = count_serialized_fields(&input.fields);
    let serialize_fields = serialize_fields(&input.fields, input.attrs.offset);

    TokenStream::from(quote! {
        impl serde::Serialize for #ident {
            fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer
            {
                use serde::ser::SerializeMap;
                let num_fields = 0
                    #(#num_fields)*
                ;
                let mut map = serializer.serialize_map(Some(num_fields))?;

                #(#serialize_fields)*

                map.end()
            }
        }
    })
}

fn none_fields(fields: &[parse::Field]) -> Vec<proc_macro2::TokenStream> {
    fields
        .iter()
        .map(|field| {
            let ident = syn::Ident::new(&field.label, proc_macro2::Span::call_site());
            quote! {
                let mut #ident = None;
            }
        })
        .collect()
}

fn unwrap_expected_fields(fields: &[parse::Field]) -> Vec<proc_macro2::TokenStream> {
    fields
        .iter()
        .map(|field| {
            let label = field.label.clone();
            let ident = syn::Ident::new(&field.label, proc_macro2::Span::call_site());
            if field.skip_serializing_if.is_none() {
                quote! {
                    let #ident = #ident.ok_or_else(|| serde::de::Error::missing_field(#label))?;
                }
            } else {
                // TODO: still confused here, but the tests pass ;)
                quote! {
                    // let #ident = #ident.or(None);
                }
            }
        })
        .collect()
}

fn match_fields(fields: &[parse::Field], offset: usize) -> Vec<proc_macro2::TokenStream> {
    fields
        .iter()
        .map(|field| {
            let label = field.label.clone();
            let ident = syn::Ident::new(&field.label, proc_macro2::Span::call_site());
            let index = field.index + offset;
            quote! {
                #index => {
                    if #ident.is_some() {
                        return Err(serde::de::Error::duplicate_field(#label));
                    }
                    #ident = Some(map.next_value()?);
                },
            }
        })
        .collect()
}

fn all_fields(fields: &[parse::Field]) -> Vec<proc_macro2::TokenStream> {
    fields
        .iter()
        .map(|field| {
            let ident = syn::Ident::new(&field.label, proc_macro2::Span::call_site());
            quote! {
                #ident
            }
        })
        .collect()
}

#[proc_macro_derive(DeserializeIndexed, attributes(serde, serde_indexed))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    let ident = input.ident;
    let none_fields = none_fields(&input.fields);
    let unwrap_expected_fields = unwrap_expected_fields(&input.fields);
    let match_fields = match_fields(&input.fields, input.attrs.offset);
    let all_fields = all_fields(&input.fields);

    TokenStream::from(quote! {
        impl<'de> serde::Deserialize<'de> for #ident {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct IndexedVisitor;

                impl<'de> serde::de::Visitor<'de> for IndexedVisitor {
                    type Value = #ident;

                    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                        formatter.write_str(stringify!(#ident))
                    }

                    fn visit_map<V>(self, mut map: V) -> Result<#ident, V::Error>
                    where
                        V: serde::de::MapAccess<'de>,
                    {
                        #(#none_fields)*

                        while let Some(key) = map.next_key()? {
                            match key {
                                #(#match_fields)*
                                _ => {
                                    return Err(serde::de::Error::duplicate_field("inexistent field index"));
                                }
                            }
                        }

                        #(#unwrap_expected_fields)*

                        Ok(#ident { #(#all_fields),* })
                    }
                }

                deserializer.deserialize_map(IndexedVisitor {})
            }
        }
    })
}