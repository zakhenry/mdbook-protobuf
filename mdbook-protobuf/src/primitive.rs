use askama::Template;
use prost_types::field_descriptor_proto::Type;
use crate::view::FieldType;

#[derive(Template)]
#[template(path = "primitive.html")]
pub(crate) struct Primitive {
    proto: &'static str,
    note: &'static str,
    cpp: &'static str,
    java_kotlin: &'static str,
    python: &'static str,
    go: &'static str,
    ruby: &'static str,
    csharp: &'static str,
    php: &'static str,
    dart: &'static str,
    rust: &'static str,
}

impl FieldType {
    // source https://github.com/protocolbuffers/protocolbuffers.github.io/blob/main/content/programming-guides/proto3.md?plain=1
    pub(crate) fn definition(&self) -> Primitive {
        match self {
            FieldType::Primitive(typ) => match typ {
                Type::Double => Primitive { proto: "double", note: "", cpp: "double", java_kotlin: "double", python: "float", go: "float64", ruby: "Float", csharp: "double", php: "float", dart: "double", rust: "f64" },
                Type::Float => Primitive { proto: "float", note: "", cpp: "float", java_kotlin: "float", python: "float", go: "float32", ruby: "Float", csharp: "float", php: "float", dart: "double", rust: "f32" },
                Type::Int32 => Primitive { proto: "int32", note: "Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint32 instead.", cpp: "int32", java_kotlin: "int", python: "int", go: "int32", ruby: "Fixnum or Bignum (as required)", csharp: "int", php: "integer", dart: "int", rust: "i32" },
                Type::Int64 => Primitive { proto: "int64", note: "Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint64 instead.", cpp: "int64", java_kotlin: "long", python: "int/long<sup>[4]</sup>", go: "int64", ruby: "Bignum", csharp: "long", php: "integer/string<sup>[6]</sup>", dart: "Int64", rust: "i64" },
                Type::Uint32 => Primitive { proto: "uint32", note: "Uses variable-length encoding.", cpp: "uint32", java_kotlin: "int", python: "int/long<sup>[4]</sup>", go: "uint32", ruby: "Fixnum or Bignum (as required)", csharp: "uint", php: "integer", dart: "int", rust: "u32" },
                Type::Uint64 => Primitive { proto: "uint64", note: "Uses variable-length encoding.", cpp: "uint64", java_kotlin: "long", python: "int/long<sup>[4]</sup>", go: "uint64", ruby: "Bignum", csharp: "ulong", php: "integer/string<sup>[6]</sup>", dart: "Int64", rust: "u64" },
                Type::Sint32 => Primitive { proto: "sint32", note: "Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int32s.", cpp: "int32", java_kotlin: "int", python: "int", go: "int32", ruby: "Fixnum or Bignum (as required)", csharp: "int", php: "integer", dart: "int", rust: "i32" },
                Type::Sint64 => Primitive { proto: "sint64", note: "Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int64s.", cpp: "int64", java_kotlin: "long", python: "int/long<sup>[4]</sup>", go: "int64", ruby: "Bignum", csharp: "long", php: "integer/string<sup>[6]</sup>", dart: "Int64", rust: "i64" },
                Type::Fixed32 => Primitive { proto: "fixed32", note: "Always four bytes. More efficient than uint32 if values are often greater than 2<sup>28</sup>.", cpp: "uint32", java_kotlin: "int", python: "int/long<sup>[4]</sup>", go: "uint32", ruby: "Fixnum or Bignum (as required)", csharp: "uint", php: "integer", dart: "int", rust: "u32" },
                Type::Fixed64 => Primitive { proto: "fixed64", note: "Always eight bytes. More efficient than uint64 if values are often greater than 2<sup>56</sup>.", cpp: "uint64", java_kotlin: "long", python: "int/long<sup>[4]</sup>", go: "uint64", ruby: "Bignum", csharp: "ulong", php: "integer/string<sup>[6]</sup>", dart: "Int64", rust: "u64" },
                Type::Sfixed32 => Primitive { proto: "sfixed32", note: "Always four bytes.", cpp: "int32", java_kotlin: "int", python: "int", go: "int32", ruby: "Fixnum or Bignum (as required)", csharp: "int", php: "integer", dart: "int", rust: "i32" },
                Type::Sfixed64 => Primitive { proto: "sfixed64", note: "Always eight bytes.", cpp: "int64", java_kotlin: "long", python: "int/long<sup>[4]</sup>", go: "int64", ruby: "Bignum", csharp: "long", php: "integer/string<sup>[6]</sup>", dart: "Int64", rust: "i64" },
                Type::Bool => Primitive { proto: "bool", note: "", cpp: "bool", java_kotlin: "boolean", python: "bool", go: "bool", ruby: "TrueClass/FalseClass", csharp: "bool", php: "boolean", dart: "bool", rust: "bool" },
                Type::String => Primitive { proto: "string", note: "A string must always contain UTF-8 encoded or 7-bit ASCII text, and cannot be longer than 2<sup>32</sup>.", cpp: "string", java_kotlin: "String", python: "str/unicode<sup>[5]</sup>", go: "string", ruby: "String (UTF-8)", csharp: "string", php: "string", dart: "String", rust: "ProtoString" },
                Type::Bytes => Primitive { proto: "bytes", note: "May contain any arbitrary sequence of bytes no longer than 2<sup>32</sup>.", cpp: "string", java_kotlin: "ByteString", python: "str (Python 2)<br/>bytes (Python 3)", go: "[]byte", ruby: "String (ASCII-8BIT)", csharp: "ByteString", php: "string", dart: "List<int>", rust: "ProtoBytes" },
                _ => panic!("typ {:?} is not scalar and should be handled separately", typ)
            },
            _ => panic!("definition is only supported for primitive fields")
        }
    }
}
