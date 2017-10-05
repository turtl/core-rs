#![recursion_limit = "512"]

extern crate proc_macro;
#[macro_use]
extern crate quote;
extern crate syn;

use std::collections::HashMap;
use proc_macro::TokenStream;

#[proc_macro_derive(Protected, attributes(protected_modeltype, protected_field))]
pub fn protected(input: TokenStream) -> TokenStream {
    let s = input.to_string();

    let ast = syn::parse_derive_input(&s).unwrap();
    let gen = impl_protected(&ast);
    gen.parse().unwrap()
}

/// Find all fields that have a serde(rename = ...) attribute and add them into
/// a original -> renamed hash.
fn field_map(body: &syn::Body, attr_type: &str, attr_name: &str) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    match body {
        &syn::Body::Struct(ref data) => {
            for field in data.fields() {
                for attr in &field.attrs {
                    match attr.value {
                        syn::MetaItem::List(ref id, ref nested) => {
                            // [MetaItem(NameValue(Ident("rename"), Str("mod", Cooked)))]
                            if id.as_ref() == attr_type {
                                for meta in nested {
                                    match meta {
                                        &syn::NestedMetaItem::MetaItem(ref meta) => {
                                            match meta {
                                                &syn::MetaItem::NameValue(ref ident, ref lit) => {
                                                    if ident.as_ref() == attr_name {
                                                        match lit {
                                                            &syn::Lit::Str(ref renamed_field, ref _style) => {
                                                                let field_str = String::from(field.ident.as_ref().unwrap().as_ref());
                                                                map.insert(field_str, renamed_field.clone());
                                                            },
                                                            _ => {},
                                                        }
                                                    }
                                                },
                                                _ => {},
                                            }
                                        },
                                        _ => {},
                                    }
                                }
                            }
                        },
                        _ => {},
                    }
                }
            }
        },
        _ => panic!("You can only use #[derive(Protected)] on Structs"),
    }
    map
}

/// Find all fields that have a serde(rename = ...) attribute and add them into
/// a original -> renamed hash.
fn find_rename_fields(body: &syn::Body) -> HashMap<String, String> {
    field_map(body, "serde", "rename")
}

/// Find all fields that have a serde(with = ...) attribute and add them into
/// a field -> converter hash.
fn find_convert_fields(body: &syn::Body) -> HashMap<String, String> {
    field_map(body, "serde", "with")
}

/// Given a list of field idents, return a list of strings that is either the
/// renamed field name (if a rename exists) or the original field name.
fn match_rename_fields(map: &HashMap<String, String>, fields: Vec<&syn::Ident>) -> Vec<String> {
    fields.into_iter()
        .map(|x| {
            let field_str: String = String::from(x.clone().as_ref());
            let matched: String = match map.get(&field_str) {
                Some(renamed) => renamed.clone(),
                None => field_str,
            };
            matched
        })
        .collect()
}

/// Finds all fields in a Struct that are marked with a
///   #[protected_field(...)]
/// meta item and match the given field type (probably either "public",
/// "private", or "submodel").
fn find_protected_fields<'a>(body: &'a syn::Body, field_type: &str, restrict: bool) -> Vec<&'a syn::Ident> {
    match body {
        &syn::Body::Struct(ref data) => {
            data.fields()
                .into_iter()
                .filter(|ref x| {
                    let mut is_pub = false;
                    // [Attribute { style: Outer, value: List(Ident("protected_field"), [MetaItem(Word(Ident("public")))]), is_sugared_doc: false }
                    for attr in &x.attrs {
                        match attr.value {
                            syn::MetaItem::List(ref id, ref nested) => {
                                if id.as_ref() == "protected_field" {
                                    if !restrict || (restrict && nested.len() <= 1) {
                                        for meta in nested {
                                            match meta {
                                                &syn::NestedMetaItem::MetaItem(ref submeta) => {
                                                    match submeta {
                                                        &syn::MetaItem::Word(ref subident) => {
                                                            if subident.as_ref() == field_type {
                                                                is_pub = true;
                                                            }
                                                        },
                                                        _ => {},
                                                    }
                                                },
                                                _ => {},
                                            }
                                        }
                                    }
                                }
                            },
                            _ => {},
                        }
                    }
                    is_pub
                })
                .map(|x| x.ident.as_ref().unwrap())
                .collect()
        },
        _ => panic!("You can only use #[derive(Protected)] on Structs"),
    }
}

fn get_struct_modeltype(attrs: &Vec<::syn::Attribute>) -> Option<String> {
    // [Attribute {
    //      style: Outer,
    //      value: List(
    //        Ident("protected_modeltype"),
    //        [MetaItem(Word(Ident("keychain")))]
    //      ),
    //      is_sugared_doc: false
    // }]
    let mut modeltype = None;
    for attr in attrs {
        match attr.value {
            ::syn::MetaItem::List(ref id, ref nested) => {
                if id.as_ref() == "protected_modeltype" {
                    for meta in nested {
                        match meta {
                            &syn::NestedMetaItem::MetaItem(ref meta) => {
                                match meta {
                                    &syn::MetaItem::Word(ref ident) => {
                                        modeltype = Some(String::from(ident.as_ref()));
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    modeltype
}

fn impl_protected(ast: &syn::MacroInput) -> quote::Tokens {
    let name = &ast.ident;
    let modeltype = get_struct_modeltype(&ast.attrs);
    let rename_field_map = find_rename_fields(&ast.body);
    let convert_field_map = find_convert_fields(&ast.body);
    let public_fields1: Vec<&syn::Ident> = find_protected_fields(&ast.body, "public", false);
    let public_fields_only: Vec<&syn::Ident> = find_protected_fields(&ast.body, "public", true);
    let public_fields_only2 = public_fields_only.clone();
    let public_fields_rename1 = match_rename_fields(&rename_field_map, public_fields1.clone());
    let public_only_fields_rename2 = match_rename_fields(&rename_field_map, public_fields_only);
    let private_fields1: Vec<&syn::Ident> = find_protected_fields(&ast.body, "private", false);
    let private_fields_only: Vec<&syn::Ident> = find_protected_fields(&ast.body, "private", true);
    let private_fields_only2 = private_fields_only.clone();
    let private_fields_rename1 = match_rename_fields(&rename_field_map, private_fields1.clone());
    let private_only_fields_rename2 = match_rename_fields(&rename_field_map, private_fields_only);
    let submodel_fields1: Vec<&syn::Ident> = find_protected_fields(&ast.body, "submodel", false);
    let submodel_fields2 = submodel_fields1.clone();
    let submodel_fields3 = submodel_fields1.clone();
    let submodel_fields4 = submodel_fields1.clone();
    let submodel_fields5 = submodel_fields1.clone();
    let submodel_fields6 = submodel_fields1.clone();
    let submodel_fields7 = submodel_fields1.clone();
    let submodel_fields8 = submodel_fields1.clone();
    let submodel_fields9 = submodel_fields1.clone();
    let submodel_fields_rename1 = match_rename_fields(&rename_field_map, submodel_fields1.clone());
    let submodel_fields_rename2 = match_rename_fields(&rename_field_map, submodel_fields1.clone());

    let des_mapper = |field: &syn::Ident| -> quote::Tokens {
        let field_name = String::from(field.as_ref());
        let field_none = field.clone();
        match convert_field_map.get(&field_name) {
            Some(x) => {
                let converter_mod: syn::Ident = From::from(x.clone());
                quote! {
                    let converted = #converter_mod::from_value(x)?;
                    if converted.is_some() {
                        self.#field = converted.unwrap();
                    };
                }
            },
            None => {
                quote! {
                    self.#field_none = ::jedi::from_val(x).map_err(|e| toterr!(e))?;
                }
            }
        }
    };
    let model_type_inner = match modeltype {
        Some(modeltype) => {
            quote! { #modeltype.to_lowercase() }
        }
        None => {
            quote! { stringify!(#name).to_lowercase() }
        }
    };
    let public_fields_merge_map: Vec<_> = public_fields_only2
        .into_iter()
        .map(&des_mapper)
        .collect();
    let private_fields_merge_map: Vec<_> = private_fields_only2
        .into_iter()
        .map(&des_mapper)
        .collect();
    quote! {
        impl ::std::fmt::Debug for #name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                let fakeid = String::from("<no id>");
                let id = match self.id() {
                    Some(x) => x,
                    None => &fakeid,
                };
                write!(f, "{}: ({})", self.model_type(), id)
            }
        }

        impl Protected for #name {
            fn key<'a>(&'a self) -> Option<&'a ::crypto::Key> {
                self._key.as_ref()
            }

            fn key_or_else(&self) -> ::error::TResult<::crypto::Key> {
                match self.key() {
                    Some(x) => Ok(x.clone()),
                    None => TErr!(::error::TError::MissingField(format!("{}.key", stringify!(#name)))),
                }
            }

            fn set_key(&mut self, key: Option<::crypto::Key>) {
                self._key = key;
                self._set_key_on_submodels();
            }

            fn model_type(&self) -> String {
                #model_type_inner
            }

            fn public_fields(&self) -> Vec<&'static str> {
                vec![
                    "id",
                    #( #public_fields_rename1, )*
                ]
            }

            fn private_fields(&self) -> Vec<&'static str> {
                vec![
                    #( #private_fields_rename1, )*
                ]
            }

            fn submodel_fields(&self) -> Vec<&'static str> {
                vec![
                    #( #submodel_fields_rename1, )*
                ]
            }

            #[allow(unused_variables)]  // required in case we have no submodels
            fn submodel_data(&self, field: &str, private: bool) -> ::error::TResult<::jedi::Value> {
                #(
                    if field == stringify!(#submodel_fields2) {
                        match self.#submodel_fields3.as_ref() {
                            Some(ref x) => {
                                return Ok(x.get_serializable_data(private)?);
                            },
                            None => return Ok(::jedi::Value::Null),
                        }
                    }
                )*
                Err(::error::TError::MissingField(format!("The field {} wasn't found in this model", field)))
            }

            fn _set_key_on_submodels(&mut self) {
                if self.key().is_none() { return; }
                #(
                    {
                        let key = self.key().unwrap().clone();
                        match self.#submodel_fields4.as_mut() {
                            Some(ref mut x) => x.set_key(Some(key)),
                            None => {},
                        }
                    }
                )*
            }

            fn serialize_submodels(&mut self) -> ::error::TResult<()> {
                #(
                    match self.#submodel_fields5.as_mut() {
                        Some(ref mut x) => {
                            x.serialize()?;
                        },
                        None => {},
                    }
                )*
                Ok(())
            }

            fn deserialize_submodels(&mut self) -> ::error::TResult<()> {
                #(
                    match self.#submodel_fields6.as_mut() {
                        Some(ref mut x) => {
                            if x.get_body().is_some() {
                                x.deserialize()?;
                            }
                        },
                        None => {},
                    }
                )*
                Ok(())
            }

            fn clone(&self) -> ::error::TResult<Self> {
                let mut model = Self::clone_from(::jedi::to_val(self).map_err(|e| toterr!(e))?)?;
                let key = self.key().map(|x| x.clone());
                model.set_key(key);
                Ok(model)
            }

            fn generate_key(&mut self) -> ::error::TResult<&::crypto::Key> {
                if self.key().is_none() {
                    let key = ::crypto::Key::random()?;
                    self.set_key(Some(key));
                }
                Ok(self.key().unwrap())
            }

            fn get_keys<'a>(&'a self) -> Option<&'a Vec<::models::keychain::KeyRef<String>>> {
                self.keys.as_ref()
            }

            fn set_keys(&mut self, keydata: Vec<::models::keychain::KeyRef<String>>) {
                self.keys = Some(keydata);
            }

            fn get_body<'a>(&'a self) -> Option<&'a String> {
                match self.body {
                    Some(ref x) => Some(x),
                    None => None,
                }
            }

            fn set_body(&mut self, body: String) {
                self.body = Some(body);
            }

            fn clear_body(&mut self) {
                self.body = None;
            }

            fn merge_fields(&mut self, data: &::jedi::Value) -> ::error::TResult<()> {
                #({
                    match ::jedi::get_opt::<::jedi::Value>(&[#public_only_fields_rename2], data) {
                        Some(x) => {
                            #public_fields_merge_map
                        },
                        _ => {},
                    }
                })*
                #({
                    match ::jedi::get_opt::<::jedi::Value>(&[#private_only_fields_rename2], data) {
                        Some(x) => {
                            #private_fields_merge_map
                        },
                        _ => {},
                    }
                })*
                #({
                    match ::jedi::get_opt::<::jedi::Value>(&[#submodel_fields_rename2], data) {
                        Some(x) => {
                            if self.#submodel_fields7.is_some() {
                                self.#submodel_fields8.as_mut().unwrap().merge_fields(&x)?;
                            } else {
                                self.#submodel_fields9 = Some(::jedi::from_val(x).map_err(|e| toterr!(e))?);
                            }
                        },
                        _ => {},
                    }
                })*
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}

