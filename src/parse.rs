use proc_macro2::Span;
use syn::meta::ParseNestedMeta;
use syn::parse::{Error, Parse, ParseStream, Result};
use syn::{Data, DeriveInput, Fields, Ident, Lifetime, LitInt, LitStr, Token};

pub struct Input {
    pub ident: Ident,
    pub attrs: StructAttrs,
    pub fields: Vec<Field>,
    pub lifetimes: Vec<Lifetime>,
}

#[derive(Default)]
pub struct StructAttrs {
    pub offset: usize,
    // pub skip_nones: bool,
}

pub struct Field {
    pub label: String,
    pub member: syn::Member,
    pub index: usize,
    pub skip_serializing_if: Option<syn::ExprPath>,
    // pub attrs: attr::Field,
    pub ty: syn::Type,
    pub original: syn::Field,
}

fn parse_meta(attrs: &mut StructAttrs, meta: ParseNestedMeta) -> Result<()> {
    if meta.path.is_ident("offset") {
        let value = meta.value()?;
        let offset: LitInt = value.parse()?;
        attrs.offset = offset.base10_parse()?;
        Ok(())
    } else {
        Err(meta.error(format_args!(
            "the only accepted struct level attribute is offset"
        )))
    }
}

fn parse_attrs(attrs: &Vec<syn::Attribute>) -> Result<StructAttrs> {
    let mut struct_attrs: StructAttrs = Default::default();

    for attr in attrs {
        if attr.path().is_ident("serde_indexed") {
            attr.parse_nested_meta(|meta| parse_meta(&mut struct_attrs, meta))?;
            // println!("parsing serde_indexed");
            // parse_meta(&mut struct_attrs, &attr.parse_meta()?)?;
        }
        if attr.path().is_ident("serde") {
            // println!("parsing serde");
            attr.parse_nested_meta(|meta| parse_meta(&mut struct_attrs, meta))?;
        }
    }

    Ok(struct_attrs)
}

fn lifetimes(generics: &syn::Generics) -> Vec<Lifetime> {
    generics.lifetimes().map(|l| l.lifetime.clone()).collect()
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        let call_site = Span::call_site();
        let derive_input = DeriveInput::parse(input)?;

        let data: syn::DataStruct = match derive_input.data {
            Data::Struct(data) => data,
            _ => {
                return Err(Error::new(call_site, "input must be a struct"));
            }
        };

        let attrs: StructAttrs = parse_attrs(&derive_input.attrs)?;

        let syn_fields: syn::FieldsNamed = match data.fields {
            Fields::Named(named_fields) => named_fields,
            _ => {
                return Err(Error::new(call_site, "struct fields must be named"));
            }
        };

        let fields = fields_from_ast(&syn_fields.named);

        let lifetimes = lifetimes(&derive_input.generics);

        //serde::internals::ast calls `fields_from_ast(cx, &fields.named, attrs, container_default)`

        Ok(Input {
            ident: derive_input.ident,
            attrs,
            fields,
            lifetimes,
        })
    }
}

fn fields_from_ast(fields: &syn::punctuated::Punctuated<syn::Field, Token![,]>) -> Vec<Field> {
    // serde::internals::ast.rs:L183
    fields
        .iter()
        .enumerate()
        .map(|(i, field)| Field {
            // these are https://docs.rs/syn/1.0.13/syn/struct.Field.html
            label: match &field.ident {
                Some(ident) => ident.to_string(),
                None => {
                    // TODO: does this happen?
                    panic!("input struct must have named fields");
                }
            },
            member: match &field.ident {
                Some(ident) => syn::Member::Named(ident.clone()),
                None => {
                    // TODO: does this happen?
                    panic!("input struct must have named fields");
                }
            },
            index: i,
            // TODO: make this... more concise? handle errors? the thing with the spans?
            skip_serializing_if: {
                let mut skip_serializing_if = None;
                for attr in &field.attrs {
                    if attr.path().is_ident("serde") {
                        attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("skip_serializing_if") {
                                let litstr: LitStr = meta
                                    .value()
                                    .expect(r#"skip_serializing_if = "literal""#)
                                    .parse()
                                    .expect(r#"skip_serializing_if = "literal""#);
                                let tokens = syn::parse_str(&litstr.value())
                                    .expect("Failed to parse attribute");
                                if skip_serializing_if.is_some() {
                                    panic!("Multiple attributes for skip_serializing_if");
                                }
                                skip_serializing_if = Some(syn::parse2(tokens).unwrap());
                                Ok(())
                            } else {
                                panic!("Unkown field attribute")
                            }
                        })
                        .expect("Failed to parse attribute");
                    }
                }
                skip_serializing_if
            },
            ty: field.ty.clone(),
            original: field.clone(),
        })
        .collect()
}
