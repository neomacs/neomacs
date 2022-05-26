use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident};

extern crate proc_macro;

#[proc_macro_derive(TryFromValue)]
pub fn derive_try_from_value(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let try_from_body = try_from_body(&name, &input.data);
    quote! {
        impl TryFrom<rmpv::Value> for #name {
            type Error = crate::error::NeomacsError;

            fn try_from(value: rmpv::Value) -> std::result::Result<Self, Self::Error> {
                #try_from_body
            }
        }
    }
    .into()
}

// TODO derive Into<Value> as well
// TODO make this work for nested Vecs and HashMaps

fn try_from_body(name: &Ident, data: &Data) -> TokenStream {
    match *data {
        Data::Struct(ref data) => {
            match data.fields {
                Fields::Named(ref fields) => {
                    let field_var_decs: Vec<TokenStream> = fields
                        .named
                        .iter()
                        .map(|f| {
                            let field_name = &f.ident;
                            let field_type = &f.ty;
                            quote! {
                                let mut #field_name: Option<#field_type> = None;
                            }
                        })
                        .collect();
                    let field_assigs: Vec<TokenStream> = fields.named.iter().map(|f| {
                        let field_name_str = f.ident.as_ref().unwrap().to_string();
                        let field_name = &f.ident;
                        quote! {
                            if k.as_str().unwrap() == #field_name_str {
                                #field_name = Some(v.clone().try_into().map_err(|_| make_err())?)
                            }
                        }
                    }).collect();
                    let field_params: Vec<TokenStream> = fields
                        .named
                        .iter()
                        .map(|f| {
                            let field_name = &f.ident;
                            quote! {
                                #field_name: #field_name.ok_or_else(make_err)?,
                            }
                        })
                        .collect();
                    let err_str = format!("Cannot parse {} from {{}}", name.to_string());
                    quote! {
                        let make_err = || {
                            crate::error::NeomacsError::MessagePackParse(format!(
                                #err_str,
                                value
                            ))
                        };
                        if !value.is_map() {
                            return Err(make_err());
                        }
                        #(#field_var_decs)*
                        for (k, v) in value.as_map().unwrap() {
                            if !k.is_str() {
                                return Err(make_err());
                            }
                            #(#field_assigs)*
                        }
                        Ok(Self {
                            #(#field_params)*
                        })
                    }
                }
                Fields::Unnamed(ref fields) => {
                    let num_fields = fields.unnamed.len();
                    let err_str = format!("Cannot parse {} from {{}}", name.to_string());
                    let field_assigs = fields.unnamed.iter().enumerate().map(|(i, _)| {
                        quote! { arr[#i].clone().try_into().map_err(|_| make_err())? }
                    });
                    quote! {
                        let make_err = || {
                            crate::error::NeomacsError::MessagePackParse(format!(
                                #err_str,
                                value
                            ))
                        };
                        if !value.is_array() {
                            return Err(make_err());
                        }
                        let arr = value.as_array().unwrap();
                        if arr.len() != #num_fields {
                            return Err(make_err());
                        }
                        return Ok(Self(#(#field_assigs),*))
                    }
                }
                Fields::Unit => unimplemented!(),
            }
        }
        _ => unimplemented!(),
    }
}
