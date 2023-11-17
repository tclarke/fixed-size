//! struct attribute to set fixed sizes for certain fields which are normally dynamic
//! 
//! This is useful when generating structs from protobufs using prost and
//! also using those structs with a serde format that requires fixed length strings
//! 
//! # Example
//! ```protoc
//! syntax = "proto3";
//! message Foo
//! {
//!  string my_string = 1;
//! }
//! ````
//! Prost will create use [`String`] for the my_string field. If you have a binary format requiring
//! exactly 4 characters in a string this will be difficult to handle in a generic manner. If you add
//! the `#[fixed(my_string=4)]` attribute then you'll end up with a `ArrayString::<4>` instead.
//! 
//! By default, ArrayString will be used but this can be overridden with `#[fixed(typ=MyString, thestring=4)]`
//! The typical use is
//! ```rust
//! use arrayvec::ArrayString;
//! 
//! struct MyString<const CAP: usize>(ArrayString<CAP>);
//! 
//! impl<const CAP: usize> AsRef<ArrayString<CAP>> for MyString<CAP> {
//!    fn as_ref(&self) -> &ArrayString<CAP> {
//!        &self.0
//!    }
//!}
//! impl<const CAP: usize> serde::Serialize for MyString<CAP> {
//!    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
//!        where S: serde::Serializer
//!    {
//!        // specialized serialize to override ArrayString's conversion to &str
//!        todo!()
//!    }
//! }
//! // More impls, probably AsMut, etc.
//! ```
//! 
//! ```rust
//! use arrayvec::ArrayString;
//! use fixed_size::fixed;
//! 
//! #[fixed(my_string=4)]
//! #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
//! struct Foo {
//!   my_string: String,
//! }
//! 
//! let foo = Foo { my_string: ArrayString::<4>::from("abcd").unwrap() };
//! // bincode actually supports var length strings but it's just used as an example and test
//! let encoded = bincode::serialize(&foo).unwrap();
//! let decoded: Foo = bincode::deserialize(&encoded[..]).unwrap();
//! assert_eq!(foo, decoded);
//! ```
//! 
//! Adding fewer than 4 characters to my_string will 0 pad the value. Adding more than
//! 4 characters will result in an error.

extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std::collections::HashMap;
use syn::{parse::{Parse, ParseStream, Result}, Token, punctuated::Punctuated,
                  fold::Fold, Expr, ExprAssign, Ident, LitInt, Lit, parse_macro_input,
                  ItemStruct, Type, Field, parse_quote};

type MapType = HashMap<Ident, LitInt>;
struct Args {
    size_map: MapType,
    typ: Ident,
}

const ERRMSG: &str = "Must specify an Ident=Int or typ=Structname";

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        let vars = Punctuated::<ExprAssign, Token![,]>::parse_terminated(input)?;
        let mut size_map = MapType::new();
        let mut typ = Ident::new("ArrayString", Span::mixed_site());
        for var in vars.into_iter() {
            match (&*var.left, &*var.right) {
                (Expr::Path(p), Expr::Lit(v)) => {
                    let key = p.path.get_ident().unwrap();
                    if let Lit::Int(num) = &v.lit {
                        size_map.insert(key.clone(), num.clone());
                    } else {
                        return Err(input.error(ERRMSG));
                    }
                },
                (Expr::Path(p), Expr::Path(v)) => {
                    let key = p.path.get_ident().unwrap();
                    if key.to_string() != "typ" {
                        return Err(input.error(ERRMSG));
                    }
                    if let Some(val) = v.path.get_ident() {
                        typ = val.clone();
                    } else {
                        return Err(input.error(ERRMSG));
                    }
                }
                (_, _) => {
                    return Err(input.error(ERRMSG));
                }
            }
        }

        Ok(Args { size_map, typ })
    }
}

impl Fold for Args {
    fn fold_field(&mut self, input: Field) -> syn::Field {
        if let Some(key) = &input.ident {
            let typ = &self.typ;
            if let Some(num) = self.size_map.get(key) {
                if let Type::Path(p) = &input.ty {
                    if p.path.is_ident("String") || p.path.segments.last().unwrap().ident.to_string() == "String" {
                        return Field {
                            attrs: input.attrs,
                            vis: input.vis,
                            mutability: input.mutability,
                            ident: input.ident,
                            colon_token: input.colon_token,
                            ty: parse_quote!{#typ::<#num>},
                        };
                    }
                }
            }
        }
        input
    }
}

/// Replace one or more variable length fields with a fixed length equivalent
/// 
/// Pass in a list of `field_name=length` arguments. Optionally
/// pass `typ=MyType` to use a different type for the replacement. See
/// the crate documentation for moreinformation.
#[proc_macro_attribute]
pub fn fixed(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut args = parse_macro_input!(args as Args);
    let input = parse_macro_input!(input as ItemStruct);
    let output = args.fold_item_struct(input);
    proc_macro::TokenStream::from(quote!(#output))
}