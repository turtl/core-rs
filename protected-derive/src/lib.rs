#![recursion_limit = "256"]

extern crate proc_macro;
#[macro_use]
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;

#[proc_macro_derive(Protected, attributes(protected_field))]
pub fn protected(input: TokenStream) -> TokenStream {
    let s = input.to_string();

    let ast = syn::parse_derive_input(&s).unwrap();
    let gen = impl_protected(&ast);
    gen.parse().unwrap()
}

/// Finds all fields in a Struct that are marked with a
///   #[protected_field(...)]
/// meta item and match the given field type (probably either "public",
/// "private", or "submodel").
fn find_protected_fields<'a>(body: &'a syn::Body, field_type: &str) -> Vec<&'a syn::Ident> {
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

fn impl_protected(ast: &syn::MacroInput) -> quote::Tokens {
    let name = &ast.ident;
    let field_idents: Vec<&syn::Ident> = match ast.body {
        syn::Body::Struct(ref data) => {
            data.fields().into_iter()
                .map(|x| x.ident.as_ref().unwrap())
                .collect()
        }
        _ => panic!("You can only use #[derive(Protected)] on Structs"),
    };
    let field_types: Vec<&syn::Ty> = match ast.body {
        syn::Body::Struct(ref data) => {
            data.fields().into_iter()
                .map(|x| &x.ty)
                .collect()
        }
        _ => panic!("You can only use #[derive(Protected)] on Structs"),
    };

    let public_fields: Vec<&syn::Ident> = find_protected_fields(&ast.body, "public");
    let private_fields: Vec<&syn::Ident> = find_protected_fields(&ast.body, "private");
    let submodel_fields1: Vec<&syn::Ident> = find_protected_fields(&ast.body, "submodel");
    let submodel_fields2: Vec<&syn::Ident> = submodel_fields1.clone();
    let submodel_fields3: Vec<&syn::Ident> = submodel_fields1.clone();
    let submodel_fields4: Vec<&syn::Ident> = submodel_fields1.clone();
    let submodel_fields5: Vec<&syn::Ident> = submodel_fields1.clone();
    let submodel_fields6: Vec<&syn::Ident> = submodel_fields1.clone();
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

            fn set_key(&mut self, key: Option<::crypto::Key>) {
                self._key = key;
                self._set_key_on_submodels();
            }

            fn model_type(&self) -> &str {
                stringify!(#name)
            }

            fn public_fields(&self) -> Vec<&'static str> {
                vec![
                    "id",
                    "body",
                    "keys",
                    #( stringify!(#public_fields), )*
                ]
            }

            fn private_fields(&self) -> Vec<&'static str> {
                vec![
                    #( stringify!(#private_fields), )*
                ]
            }

            fn submodel_fields(&self) -> Vec<&'static str> {
                vec![
                    #( stringify!(#submodel_fields1), )*
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
                let mut model = Model::clone_from::<Self>(::jedi::to_val(self).map_err(|e| toterr!(e)))?;
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

            fn get_keys<'a>(&'a self) -> Option<&'a Vec<::std::collections::HashMap<String, String>>> {
                match self.keys {
                    Some(ref x) => Some(x),
                    None => None,
                }
            }

            fn set_keys(&mut self, keydata: Vec<::std::collections::HashMap<String, String>>) {
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
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}

