//! This module provides helpers/macros for serializing (mainyl for structs).
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

/// Define a generic class others can use for serialization. Not sure why serde
/// doesn't just define a basic one for us, but here it is.
pub struct TurtlMapVisitor<'a, T: 'a> {
    pub value: &'a T,
    pub state: u8,
}

/// Given a &str value, checks to see if it matches "type_", and if so returns
/// "type" instead (otherwise just returns the value).
///
/// This is useful for translating between rust structs, which don't allow a
/// field named `type` and our JSON objects out in the wild, many of which *do*
/// have a `type` field.
#[macro_export]
macro_rules! fix_type {
    ($val:expr) => {
        {
            let myval = $val;
            match myval {
                "type_" => "type",
                _ => myval
            }
        }
    }
}

/// Given a set of fields, ie:
///
///     (id, name, ...)
///
/// Define a set of `if` statements that map the current value being iterated on
/// (recursively, since macros can't count) to a set of serializer calls. This
/// is probably gibberish, so for example calling:
///
/// ```
/// define_ser_fields!(0, id, name, title)
/// ```
///
/// expands to:
///
/// ```
/// if(self.state == 0) {
///     return Ok(Some(try!(serializer.serialize_struct_elt("id", &self.value.id))))
/// }
/// if(self.state == 1) {
///     return Ok(Some(try!(serializer.serialize_struct_elt("name", &self.value.name))))
/// }
/// if(self.state == 2) {
///     return Ok(Some(try!(serializer.serialize_struct_elt("title", &self.value.title))))
/// }
/// if(self.state > $val) { return Ok(None) }
/// ```
///
/// I'd use `match` instead of a bunch of ifs but I'm not sure how to make it
/// work recursively, and also it doesn't like the `0+1+1` syntax used to count
/// the recursion.
///
/// See [the serde serialization docs](https://github.com/serde-rs/serde#serialization-without-macros)
/// if you need info one why this would be useful.
#[macro_export]
macro_rules! define_ser_fields {
    // end of sequence (last element)
    ( $val:expr, $this:ident, $ser:ident, $name:ident ) => {
        if $this.state == $val {
            $this.state += 1;
            let field_str = fix_type!(stringify!($name));
            return Ok(Some(try!($ser.serialize_struct_elt(field_str, &$this.value.$name))))
        }
        if $this.state > $val { return Ok(None) }
    };
    ( $val:expr, $this:ident, $ser:ident, $name:ident, $( $recname:ident ),* ) => {
        if $this.state == $val {
            $this.state += 1;
            let field_str = fix_type!(stringify!($name));
            return Ok(Some(try!($ser.serialize_struct_elt(field_str, &$this.value.$name))))
        }
        define_ser_fields!( 1 + $val, $this, $ser, $( $recname ),* );
    }
}

/// Implements the Serialize/Deserialize traits for our struct.
/// 
/// Don't use this directly, use serializable!()
#[macro_export]
macro_rules! serializable_impl {
    (
        $name:ident,
        ( $( $field:ident ),* )
    ) => {
        impl ::serde::ser::Serialize for $name {
            fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
                where S: ::serde::ser::Serializer
            {
                serializer.serialize_struct(stringify!($name), ::util::serialization::TurtlMapVisitor {
                    value: self,
                    state: 0
                })
            }
        }

        // Implement our serializer for this struct
        impl<'a> ::serde::ser::MapVisitor for ::util::serialization::TurtlMapVisitor<'a, $name> {
            fn visit<S>(&mut self, serializer: &mut S) -> Result<Option<()>, S::Error>
                where S: ::serde::ser::Serializer
            {
                define_ser_fields!(0, self, serializer, $( $field ),*);
                Ok(None)
            }
        }
    }
}

/// Define a class as serializable. Takes the struct name and the serializable
/// fields for that class and writes a set of functionality to make it
/// serde-serializable.
///
/// Note that this also makes the class *de*serializable as well. IF you want
/// one, you generally want the other.
///
/// TODO: Fix the crappy duplication in the macro. There are four variants that
/// I'm convinced could be (at most) two variants, but Rust's macro system is...
/// immature. Revisit in a year.
#[macro_export]
macro_rules! serializable {
    // pub/unserialized
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            ($( $unserialized:ident: $unserialized_type:ty ),*)
            $(
                //$(#[$field_meta:meta])*
                $field:ident: $type_:ty,
            )*
        }
    ) => {
        $(#[$struct_meta])*
        pub struct $name {
            $(
               //$(#[$field_meta])*
               $unserialized: $unserialized_type,
            )*
            $(
               //$(#[$field_meta])*
               $field: $type_
            ),* ,
        }

        serializable_impl!($name, ($( $field ),*));
    };

    // pub/no unserialized
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            $(
                //$(#[$field_meta:meta])*
                $field:ident: $type_:ty,
            )*
        }
    ) => {
        $(#[$struct_meta])*
        pub struct $name {
            $(
               //$(#[$field_meta])*
               $field: $type_
            ),* ,
        }

        serializable_impl!($name, ($( $field ),*));
    };

    // no pub/unserialized
    (
        $(#[$struct_meta:meta])*
        struct $name:ident {
            ($( $unserialized:ident: $unserialized_type:ty ),*)
            $(
                //$(#[$field_meta:meta])*
                $field:ident: $type_:ty,
            )*
        }
    ) => {
        $(#[$struct_meta])*
        struct $name {
            $(
               //$(#[$field_meta])*
               $unserialized: $unserialized_type,
            )*
            $(
               //$(#[$field_meta])*
               $field: $type_
            ),* ,
        }

        serializable_impl!($name, ($( $field ),*));
    };

    // no pub/no unserialized
    (
        $(#[$struct_meta:meta])*
        struct $name:ident {
            $(
                //$(#[$field_meta:meta])*
                $field:ident: $type_:ty,
            )*
        }
    ) => {
        $(#[$struct_meta])*
        struct $name {
            $(
               //$(#[$field_meta])*
               $field: $type_
            ),* ,
        }

        serializable_impl!($name, ($( $field ),*));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::util::json::{self};

    serializable!{
        #[allow(dead_code)]
        /// Our little crapper. He sometimes craps his pants.
        pub struct LittleCrapper {
            (active: bool)
            // broken for now, see https://github.com/rust-lang/rust/issues/24827
            //#[allow(dead_code)]
            name: String,
            location: String,
        }
    }

    #[test]
    fn can_serialize() {
        let crapper = LittleCrapper { active: false, name: String::from("barry"), location: String::from("my pants") };
        let json_str = json::stringify(&crapper).unwrap();
        assert_eq!(json_str, r#"{"name":"barry","location":"my pants"}"#);
    }
}

