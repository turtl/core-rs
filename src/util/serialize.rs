//! This module provides helpers/macros for serializing (mainly for structs).
//! Note this is all more or less written as a replacement for the derive()
//! attributes that no longer work in serde:
//!
//! ```
//! #[derive(Serialize, Deserialize)]
//! struct MyStruct {}
//! ```
//!
//! ...lame. Surely, the next version of Rust will support this, rendering all
//! my code useless. But hey, it's a really good test of my macro skills.

/// Define a generic struct we can use for deserialization.
///
/// Note that we actually need the T specifier so we can implement this struct
/// for other structs without causing duplicate errors. This class would not
/// be needed in the first place if we had gensyms or ident concatenation in
/// rust macros. sigh. That said, the `value` field is only ever initialized
/// with a `None` value, so this shouldn't be too expensive.
pub struct TurtlVisitor<'a, T: 'a> {
    pub value: Option<&'a T>
}

/// Given a &str value, checks to see if it matches "type_", and if so returns
/// "type" instead. It also does the reverse: if it detects "type", it returns
/// "type_". That way we can use this 
///
/// This is useful for translating between rust structs, which don't allow a
/// field named `type` and our JSON objects out in the wild, many of which *do*
/// have a `type` field.
///
/// This now also applies to `mod`, apparently.
#[macro_export]
macro_rules! fix_type {
    ( "mod" ) => { "mod_" };
    ( "mod_" ) => { "mod" };
    ( "type" ) => { "type_" };
    ( "type_" ) => { "type" };
    ( $val:expr ) => {
        {
            let myval = $val;
            match myval {
                "type_" => "type",
                "type" => "type_",
                "mod_" => "mod",
                "mod" => "mod_",
                _ => myval,
            }
        }
    }
}

/// Define a struct as serializable. Takes the struct name and the serializable
/// fields for that struct and writes a set of functionality to make it
/// serde-serializable.
///
/// Note that this also makes the struct *de*serializable as well. IF you want
/// one, you generally want the other.
///
/// TODO: Fix the crappy duplication in the macro. There are four variants that
/// I'm convinced could be (at most) two variants, but Rust's macro system is...
/// immature. Revisit in a year.
/// TODO: If rust fixes its macro system to allow ident contatenation or gensyms
/// then we can fix some issues in the deserializer implementation that allocs
/// String types where an enum would be more efficient (as noted in [the serde
/// deserialization guide](https://github.com/serde-rs/serde#deserialization-without-macros)).
#[macro_export]
macro_rules! serializable {
    // pub w/ unserialized
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            ($( $unserialized:ident: $unserialized_type:ty ),*)
            $( $field:ident: $type_:ty, )*
        }
    ) => {
        serializable!([IMPL ($name), ($( $field: $type_ ),*), ($( $unserialized: $unserialized_type ),*), (
            ($(#[$struct_meta])*)
            pub struct $name {
                $( $unserialized: $unserialized_type, )*
                $( $field: $type_ ),* ,
            }
        )]);
    };

    // pub w/ no unserialized
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            $( $field:ident: $type_:ty, )*
        }
    ) => {
        serializable!([IMPL ($name), ($( $field: $type_ ),*), (), (
            ($(#[$struct_meta])*)
            pub struct $name {
                $( $field: $type_ ),* ,
            }
        )]);
    };

    // no pub w/ unserialized
    (
        $(#[$struct_meta:meta])*
        struct $name:ident {
            ($( $unserialized:ident: $unserialized_type:ty ),*)
            $( $field:ident: $type_:ty, )*
        }
    ) => {
        serializable!([IMPL ($name), ($( $field: $type_ ),*), ($( $unserialized: $unserialized_type ),*), (
            ($(#[$struct_meta])*)
            struct $name {
                $( $unserialized: $unserialized_type, )*
                $( $field: $type_ ),* ,
            }
        )]);
    };

    // no pub w/ no unserialized
    (
        $(#[$struct_meta:meta])*
        struct $name:ident {
            $( $field:ident: $type_:ty, )*
        }
    ) => {
        serializable!([IMPL ($name), ($( $field: $type_ ),*), (), (
            ($(#[$struct_meta])*)
            struct $name {
                $( $field: $type_ ),* ,
            }
        )]);
    };

    // implementation
    (
        [IMPL ( $name:ident ), ( $( $field:ident: $type_:ty ),* ), ($( $unserialized:ident: $unserialized_type:ty ),*), (
            ($(#[$struct_meta:meta])*)
            $thestruct:item
        )]
    ) => {
        $(#[$struct_meta])*
        $thestruct

        impl ::serde::ser::Serialize for $name {
            fn serialize<S>(&self, serializer: &mut S) -> ::std::result::Result<(), S::Error>
                where S: ::serde::ser::Serializer
            {
                let mut state = try!(serializer.serialize_struct(stringify!($name), 1));
                $( try!(serializer.serialize_struct_elt(&mut state, fix_type!(stringify!($field)), &self.$field)); )*
                serializer.serialize_struct_end(state)
            }
        }

        impl ::serde::de::Deserialize for $name {
            fn deserialize<D>(deserializer: &mut D) -> Result<$name, D::Error>
                where D: ::serde::de::Deserializer
            {
                static FIELDS: &'static [&'static str] = &[ $( stringify!($field) ),* ];
                let val: Option<&$name> = None;
                deserializer.deserialize_struct(stringify!($name), FIELDS, ::util::serialize::TurtlVisitor { value: val })
            }
        }

        impl<'a> ::serde::de::Visitor for ::util::serialize::TurtlVisitor<'a, $name> {
            type Value = $name;

            fn visit_map<V>(&mut self, mut visitor: V) -> Result<$name, V::Error>
                where V: ::serde::de::MapVisitor
            {
                $( let mut $field: Option<$type_> = None; )*
                loop {
                    // note that instead of using an Enum to ENUMerate our
                    // struct fields, we just use strings. this makes things a
                    // lot easier to macro away (since we don't have to deal
                    // with passing enum names down or putting things in sub
                    // modules and digging into them to get the struct we want).
                    //
                    // if rust's macro system were a bit more robust, ie we
                    // could do things like
                    //
                    // macro_rules! shit {
                    //     ($name:ident) => {
                    //         enum FieldsFor${name} {}
                    //         ...
                    //     }
                    // }
                    //
                    // (or if we just had gensyms)
                    //
                    // none of this would be necessary. as mentioned, we could
                    // just pass down the name in the macro itself, but this is
                    // unwieldy and i'd much rather take the performance hit and
                    // keep the API clean.
                    let val: Option<String> = try!(visitor.visit_key());
                    match val {
                        Some(x) => {
                            let mut was_set = false;
                            $(
                                let fieldname = fix_type!(stringify!($field));
                                if x == fieldname {
                                    $field = Some(try!(visitor.visit_value()));
                                    was_set = true;
                                };
                            )*
                            // serde doesn't like when you don't actually use a
                            // value. it won't stand for it.
                            if !was_set {
                                drop(visitor.visit_value::<()>());
                            }
                        },
                        None => break,
                    };
                }

                $(
                    let $field: $type_ = match $field {
                        Some(x) => x,
                        None => Default::default(),
                        //None => try!(visitor.missing_field(stringify!($field))),
                    };
                )*

                try!(visitor.end());
                Ok($name{
                    $( $field: $field, )*
                    $( $unserialized: Default::default(), )*
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ::jedi::{self};

    serializable!{
        #[allow(dead_code)]
        #[derive(Debug)]
        /// Our little crapper. He sometimes craps his pants.
        pub struct LittleCrapper {
            (active: bool)
            // broken for now, see https://github.com/rust-lang/rust/issues/24827
            //#[allow(dead_code)]
            name: String,
            type_: String,
            location: String,
        }
    }

    impl LittleCrapper {
        fn new(name: String, location: String) -> LittleCrapper {
            LittleCrapper {
                name: name,
                type_: String::from("sneak"),
                location: location,
                active: true
            }
        }
    }

    serializable!{
        // let's make a recursive structure!
        struct CrapTree {
            name: String,
            crappers: Vec<LittleCrapper>,
        }
    }

    #[test]
    fn fixes_types() {
        assert_eq!(fix_type!("type"), "type_");
        assert_eq!(fix_type!("type_"), "type");
        assert_eq!(fix_type!("tpye"), "tpye");
        assert_eq!(fix_type!("stop ignoring me"), "stop ignoring me");
        assert_eq!(fix_type!(stringify!(type)), "type_");

        match "type" {
            fix_type!("type_") => {},
            _ => panic!("bad `type` match"),
        }
    }

    #[test]
    fn can_serialize() {
        let crapper = LittleCrapper { active: false, name: String::from("barry"), type_: String::from("speedy"), location: String::from("my pants") };
        let json_str = jedi::stringify(&crapper).unwrap();
        assert_eq!(json_str, r#"{"name":"barry","type":"speedy","location":"my pants"}"#);
    }

    #[test]
    fn can_deserialize() {
        let crapper: LittleCrapper = jedi::parse(&String::from(r#"{"name":"omg","location":"city hall"}"#)).unwrap();
        assert_eq!(crapper.name, "omg");
        assert_eq!(crapper.type_, "");
        assert_eq!(crapper.location, "city hall");
        assert_eq!(crapper.active, false);
    }

    #[test]
    fn can_recurse() {
        let tree = CrapTree {
            name: String::from("tree of crappy wisdom"),
            crappers: vec![
                LittleCrapper::new(String::from("harold"), String::from("here")),
                LittleCrapper::new(String::from("sandra"), String::from("the bed"))
            ]
        };
        let json_str = jedi::stringify(&tree).unwrap();
        assert_eq!(json_str, r#"{"name":"tree of crappy wisdom","crappers":[{"name":"harold","type":"sneak","location":"here"},{"name":"sandra","type":"sneak","location":"the bed"}]}"#);
    }
}

