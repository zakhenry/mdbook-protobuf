use std::any::Any;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Error, Result};
use bytes::Bytes;
use prost::Message;
use prost_types::{DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet, ServiceDescriptorProto};

pub fn read_file_descriptor_set(path: &Path) -> Result<FileDescriptorSet> {
    let mut file = File::open(path)?;

    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer)?;

    let bytes = Bytes::from(buffer);

    Ok(FileDescriptorSet::decode(bytes)?)
}

use std::fs::canonicalize;
use anyhow::anyhow;
use log::{debug, info};
use mdbook::book::{Book, Chapter};
use mdbook::BookItem;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};

/// A no-op preprocessor.
pub struct ProtobufPreprocessor;

impl ProtobufPreprocessor {
    pub fn new() -> ProtobufPreprocessor {
        ProtobufPreprocessor
    }
}

impl Preprocessor for ProtobufPreprocessor {
    fn name(&self) -> &str {
        "protobuf"
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book, Error> {
        let config = ctx.config.get_preprocessor(self.name()).ok_or(anyhow!("Expected config"))?;

        let file_descriptor_path = config.get("proto_descriptor").ok_or(anyhow!("expected `proto_descriptor` key in config"))?;

        let mut path = ctx.root.clone();
        path.push(file_descriptor_path.as_str().ok_or(anyhow!("`proto_descriptor` should be a string"))?);

        let file_descriptor_path = canonicalize(path)?;

        info!("fd: {:?}", file_descriptor_path);

        let file_descriptor_set = read_file_descriptor_set(file_descriptor_path.as_path())?;

        info!("found {} proto files", file_descriptor_set.file.len());

        for file_descriptor in file_descriptor_set.file {
            let name = format!("{}", &file_descriptor.package());
            let path = PathBuf::from(format!("proto/{}", &file_descriptor.name()));

            let content = render_protobuf_descriptor(file_descriptor)?;

            let section = BookItem::Chapter(Chapter::new(
                name.as_ref(),
                content,
                path,
                Vec::new(),
            ));
            book.sections.push(section);
        }

        Ok(book)
    }

    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer != "not-supported"
    }
}

use askama::Template;
use prost_types::field_descriptor_proto::Type;
use prost_types::source_code_info::Location;

#[derive(Template)]
#[template(path = "message_link.html")]
struct ProtoMessageLink {
    label: String,
}

impl ProtoMessageLink {
    fn from_type_name(type_name: String) -> Self {
        Self {
            label: type_name,
        }
    }

    fn href(&self) -> String {
        format!("/path/to/{}", self.label)
    }
}

#[derive(Template)]
#[template(path = "enum_link.html")]
struct EnumLink {
    label: String,
}

impl EnumLink {
    fn from_type_name(type_name: String) -> Self {
        Self {
            label: type_name,
        }
    }

    fn href(&self) -> String {
        format!("/path/to/{}", self.label)
    }
}

enum FieldType {
    Message(ProtoMessageLink),
    Enum(EnumLink),
    Primitive(Type),
    Unimplemented,
}

#[derive(Template)]
#[template(path = "field.html")]
struct Field {
    name: String,
    meta: Option<Location>,
    typ: FieldType,
}

impl Field {
    fn from_descriptor(file_descriptor: &FileDescriptorProto, field_descriptor: &FieldDescriptorProto, path: &[i32]) -> Self {
        Self {
            name: field_descriptor.name().into(),
            meta: read_source_code_info(file_descriptor, path),
            typ: match field_descriptor.r#type {
                None => {
                    FieldType::Unimplemented // todo look up fully qualified from index.
                }
                Some(label) => {
                    match Type::try_from(label).expect("should be of type") {
                        Type::Enum => FieldType::Enum(EnumLink::from_type_name(field_descriptor.type_name.clone().unwrap())),
                        Type::Message => FieldType::Message(ProtoMessageLink::from_type_name(field_descriptor.type_name.clone().unwrap())),
                        t => FieldType::Primitive(t),
                    }
                }
            },
        }
    }
}

#[derive(Template)]
#[template(path = "message.html")]
struct ProtoMessage {
    name: String,
    meta: Option<Location>,
    nested_message: Vec<ProtoMessage>,
    nested_enum: Vec<Enum>,
    fields: Vec<Field>,
}

impl ProtoMessage {
    fn from_descriptor(file_descriptor: &FileDescriptorProto, message_descriptor: &DescriptorProto, path: &[i32]) -> Self {
        Self {
            name: message_descriptor.name().into(),
            meta: read_source_code_info(file_descriptor, path),
            nested_message: message_descriptor.nested_type.iter().enumerate().map(|(idx, m)| {
                let mut nested_path = path.to_vec();
                nested_path.extend(&[idx as i32]);
                ProtoMessage::from_descriptor(file_descriptor, m, nested_path.as_ref())
            }).collect(),
            nested_enum: message_descriptor.enum_type.iter().enumerate().map(|(idx, m)| {
                let mut nested_path = path.to_vec();
                nested_path.extend(&[idx as i32]);
                Enum::from_descriptor(file_descriptor, m, nested_path.as_ref())
            }).collect(),
            fields: message_descriptor.field.iter().enumerate().map(|(idx, f)| {
                let mut nested_path = path.to_vec();
                nested_path.extend(&[SERVICE_METHOD_TAG, idx as i32]);
                Field::from_descriptor(file_descriptor, f, nested_path.as_ref())
            }).collect(),
        }
    }

    fn href_id(&self) -> String {
        self.name.clone()
    }
}


#[derive(Template)]
#[template(path = "enum.html")]
struct Enum {
    name: String,
    meta: Option<Location>,
    values: Vec<String>,
}

impl Enum {
    fn from_descriptor(file_descriptor: &FileDescriptorProto, enum_descriptor: &EnumDescriptorProto, path: &[i32]) -> Self {
        Self {
            name: enum_descriptor.name().into(),
            meta: read_source_code_info(file_descriptor, path),
            values: enum_descriptor.value.iter().map(|v| v.name().to_string()).collect(),
        }
    }

    fn href_id(&self) -> String {
        self.name.clone()
    }
}

#[derive(Template)]
#[template(path = "method.html")]
struct Method {
    name: String,
    request_message: ProtoMessageLink,
    response_message: ProtoMessageLink,
    meta: Option<Location>,
}

#[derive(Template)]
#[template(path = "service.html")]
struct Service {
    name: String,
    methods: Vec<Method>,
    meta: Option<Location>,
}

#[derive(Template)]
#[template(path = "proto.html")]
struct ProtoFileTemplate {
    services: Vec<Service>,
    messages: Vec<ProtoMessage>,
    enums: Vec<Enum>,
}

// these tags come from FileDescriptorProto - prost doesn't provide a way to read this as-yet
// see https://github.com/tokio-rs/prost/issues/137 const SERVICE_METHOD_TAG: i32 = 2;
const DESCRIPTOR_FIELD_TAG: i32 = 2;
const SERVICE_METHOD_TAG: i32 = 2;
const MESSAGE_TYPE_TAG: i32 = 4;
const ENUM_TYPE_TAG: i32 = 5;
const SERVICE_TAG: i32 = 6;

fn read_source_code_info(descriptor: &FileDescriptorProto, path: &[i32]) -> Option<Location> {
    if let Some(info) = &descriptor.source_code_info {
        info.location.iter().find(|location| location.path == path).cloned()
    } else {
        None
    }
}

fn render_protobuf_descriptor(descriptor: FileDescriptorProto) -> Result<String> {
    let services = descriptor.service.iter().enumerate().map(|(service_idx, s)| Service {
        name: s.name().into(),
        methods: s.method.iter().enumerate().map(|(method_idx, m)| Method {
            name: m.name().parse().unwrap(),
            request_message: ProtoMessageLink::from_type_name(m.input_type.clone().unwrap()),
            response_message: ProtoMessageLink::from_type_name(m.output_type.clone().unwrap()),
            meta: read_source_code_info(&descriptor, &[SERVICE_TAG, service_idx as i32, SERVICE_METHOD_TAG, method_idx as i32]),
        }).collect(),
        meta: read_source_code_info(&descriptor, &[SERVICE_TAG, service_idx as i32]),
    }).collect();

    let messages = descriptor.message_type.iter().enumerate().map(|(message_idx, m)| ProtoMessage::from_descriptor(&descriptor, m, &[MESSAGE_TYPE_TAG, message_idx as i32])).collect();

    let enums = descriptor.enum_type.iter().enumerate().map(|(enum_idx, e)| Enum::from_descriptor(&descriptor, e, &[ENUM_TYPE_TAG, enum_idx as i32])).collect();

    let view = ProtoFileTemplate { services, messages, enums };

    Ok(view.render()?)
}

#[cfg(test)]
mod test {
    use super::*;


    #[test]
    fn it_should_read_proto_descriptor() {
        let path = Path::new("../demo/docs/build/proto_file_descriptor_set.pb");
        let descriptor = read_file_descriptor_set(path);

        assert!(descriptor.is_ok());
        dbg!(&descriptor);
    }

    #[test]
    fn preprocessor_run() {
        let input_json = r##"[
                {
                    "root": "./",
                    "config": {
                        "book": {
                            "authors": ["AUTHOR"],
                            "language": "en",
                            "multilingual": false,
                            "src": "src",
                            "title": "TITLE"
                        },
                        "preprocessor": {
                            "protobuf": {
                                "proto_descriptor": "../demo/docs/build/proto_file_descriptor_set.pb"
                            }
                        }
                    },
                    "renderer": "html",
                    "mdbook_version": "0.4.21"
                },
                {
                    "sections": [
                        {
                            "Chapter": {
                                "name": "Chapter 1",
                                "content": "# Chapter 1\n",
                                "number": [1],
                                "sub_items": [],
                                "path": "chapter_1.md",
                                "source_path": "chapter_1.md",
                                "parent_names": []
                            }
                        }
                    ],
                    "__non_exhaustive": null
                }
            ]"##;
        let input_json = input_json.as_bytes();

        let (ctx, book) = mdbook::preprocess::CmdPreprocessor::parse_input(input_json).unwrap();
        let expected_book = book.clone();
        let result = ProtobufPreprocessor::new().run(&ctx, book);
        assert!(result.is_ok());
    }
}

