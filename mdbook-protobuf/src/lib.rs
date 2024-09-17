use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::Into;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Error, Result};
use bytes::Bytes;
use prost::Message;
use prost_types::{
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    FileDescriptorSet, ServiceDescriptorProto,
};

pub fn read_file_descriptor_set(path: &Path) -> Result<FileDescriptorSet> {
    let mut file = File::open(path)?;

    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer)?;

    let bytes = Bytes::from(buffer);

    Ok(FileDescriptorSet::decode(bytes)?)
}

use anyhow::anyhow;
use askama::filters::format;
use log::{debug, info, warn};
use mdbook::book::{Book, Chapter, SectionNumber};
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::BookItem;
use std::fs::canonicalize;

const PREPROCESSOR_NAME: &'static str = "protobuf";

pub struct ProtobufPreprocessor;

impl ProtobufPreprocessor {
    pub fn new() -> ProtobufPreprocessor {
        ProtobufPreprocessor
    }
}

pub struct ProtobufPreprocessorArgs {
    nest_under: Option<String>,
    file_descriptor_path: PathBuf,
}

impl ProtobufPreprocessorArgs {
    pub fn new(ctx: &PreprocessorContext) -> Result<Self> {
        let config = ctx
            .config
            .get_preprocessor(PREPROCESSOR_NAME)
            .ok_or(anyhow!("Expected config"))?;

        let file_descriptor_path = config
            .get("proto_descriptor")
            .ok_or(anyhow!("expected `proto_descriptor` key in config"))?;

        let mut path = ctx.root.clone();
        path.push(
            file_descriptor_path
                .as_str()
                .ok_or(anyhow!("`proto_descriptor` should be a string"))?,
        );

        let file_descriptor_path = canonicalize(path)?;

        Ok(Self {
            file_descriptor_path,
            nest_under: config
                .get("nest_under")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
        })
    }
}

impl Preprocessor for ProtobufPreprocessor {
    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book, Error> {
        info!("book: {:?}", book);

        let args = ProtobufPreprocessorArgs::new(ctx)?;

        info!("fd: {:?}", args.file_descriptor_path);

        let file_descriptor_set = read_file_descriptor_set(args.file_descriptor_path.as_path())?;

        info!("found {} proto files", file_descriptor_set.file.len());

        let mut namespaces: BTreeMap<String, ProtoNamespaceTemplate> = BTreeMap::new();

        let packages: HashSet<String> = file_descriptor_set
            .file
            .iter()
            .map(|f| f.package().to_string())
            .collect();

        for file_descriptor in file_descriptor_set.file {
            let value = namespaces
                .entry(file_descriptor.package().to_string())
                .or_default();
            value
                .files
                .push(ProtoFileDescriptorTemplate::from_descriptor(
                    file_descriptor,
                    &packages,
                ));
        }

        // @todo support searching sub chapters
        let target_chapter = if let Some(nest_under) = args.nest_under {
            let found_section = book.sections.iter_mut().find_map(|s| match s {
                BookItem::Chapter(c) => {
                    if c.name == nest_under {
                        Some(c)
                    } else {
                        None
                    }
                }
                _ => None,
            });

            if let None = found_section {
                warn!("`nest_under` config was defined, but no chapter matching name `{}` was found. Note nested chapters are not yet supported.", nest_under);
            }

            found_section
        } else {
            None
        };

        let chapters: Result<Vec<Chapter>> = namespaces
            .iter()
            .map(|(namespace_key, namespace)| {
                let content = namespace.render()?;
                let path = PathBuf::from(format!("proto/{}", &namespace_key.replace(".", "/")));
                Ok(Chapter::new(
                    namespace_key.as_ref(),
                    content,
                    path,
                    Vec::new(),
                ))
            })
            .collect();

        if let Some(target) = target_chapter {
            for (idx, mut chapter) in chapters?.into_iter().enumerate() {
                let mut section_number = target.clone().number.unwrap().0;
                section_number.push((idx + 1) as u32);
                chapter.number = Some(SectionNumber(section_number));
                chapter.parent_names.extend(target.parent_names.clone());
                chapter.parent_names.push(target.name.clone());

                let section = BookItem::Chapter(chapter);

                target.sub_items.push(section);
            }
        } else {
            book.sections
                .extend(chapters?.into_iter().map(BookItem::Chapter));
        }

        Ok(book)
    }

    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer != "not-supported"
    }
}

use askama::Template;
use clap::arg;
use prost_types::field_descriptor_proto::Type;
use prost_types::source_code_info::Location;

#[derive(Template)]
#[template(path = "symbol_link.html")]
struct SymbolLink {
    label: String,
    id: String,
    path: String,
}

impl SymbolLink {
    fn from_type_name(type_name: String, packages: &HashSet<String>) -> Self {
        let label = if let Some(index) = &type_name.rfind('.') {
            &type_name[index + 1..]
        } else {
            &type_name
        }
        .into();

        let best_match = packages
            .iter()
            .filter(|value| type_name[1..].starts_with(value.as_str()))
            .max_by_key(|value| value.len());

        if let Some(path) = best_match {
            Self {
                label,
                id: type_name[1..].replace(&format!("{}.", path), ""),
                path: path.to_string().replace(".", "/"),
            }
        } else {
            Self {
                label,
                id: type_name[1..].to_string(),
                path: "".into(),
            }
        }
    }

    fn href(&self) -> String {
        format!("/proto/{}.md#{}", self.path, self.id)
    }
}

enum FieldType {
    Symbol(SymbolLink),
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
    fn from_descriptor(
        file_descriptor: &FileDescriptorProto,
        field_descriptor: &FieldDescriptorProto,
        path: &[i32],
        packages: &HashSet<String>,
    ) -> Self {
        Self {
            name: field_descriptor.name().into(),
            meta: read_source_code_info(file_descriptor, path),
            typ: match field_descriptor.r#type {
                None => {
                    FieldType::Unimplemented // todo look up fully qualified from index.
                }
                Some(label) => match Type::try_from(label).expect("should be of type") {
                    Type::Enum | Type::Message => FieldType::Symbol(SymbolLink::from_type_name(
                        field_descriptor.type_name().to_string(),
                        packages,
                    )),
                    t => FieldType::Primitive(t),
                },
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
    namespace: Vec<String>,
}

impl ProtoMessage {
    fn from_descriptor(
        file_descriptor: &FileDescriptorProto,
        message_descriptor: &DescriptorProto,
        source_path: &[i32],
        namespace_path: Vec<String>,
        packages: &HashSet<String>,
    ) -> Self {
        let mut nested_namespace = namespace_path.clone();
        nested_namespace.push(message_descriptor.name().into());

        Self {
            name: message_descriptor.name().into(),
            namespace: namespace_path,
            meta: read_source_code_info(file_descriptor, source_path),
            nested_message: message_descriptor
                .nested_type
                .iter()
                .enumerate()
                .map(|(idx, m)| {
                    let mut nested_path = source_path.to_vec();
                    nested_path.extend(&[idx as i32]);
                    ProtoMessage::from_descriptor(
                        file_descriptor,
                        m,
                        nested_path.as_ref(),
                        nested_namespace.clone(),
                        packages,
                    )
                })
                .collect(),
            nested_enum: message_descriptor
                .enum_type
                .iter()
                .enumerate()
                .map(|(idx, m)| {
                    let mut nested_path = source_path.to_vec();
                    nested_path.extend(&[idx as i32]);
                    Enum::from_descriptor(
                        file_descriptor,
                        m,
                        nested_path.as_ref(),
                        nested_namespace.clone(),
                    )
                })
                .collect(),
            fields: message_descriptor
                .field
                .iter()
                .enumerate()
                .map(|(idx, f)| {
                    let mut nested_path = source_path.to_vec();
                    nested_path.extend(&[SERVICE_METHOD_TAG, idx as i32]);
                    Field::from_descriptor(file_descriptor, f, nested_path.as_ref(), packages)
                })
                .collect(),
        }
    }

    fn href_id(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.namespace.join("."), self.name)
        }
    }
}

#[derive(Template)]
#[template(path = "enum.html")]
struct Enum {
    name: String,
    meta: Option<Location>,
    values: Vec<String>,
    namespace: Vec<String>,
}

impl Enum {
    fn from_descriptor(
        file_descriptor: &FileDescriptorProto,
        enum_descriptor: &EnumDescriptorProto,
        path: &[i32],
        namespace: Vec<String>,
    ) -> Self {
        Self {
            name: enum_descriptor.name().into(),
            meta: read_source_code_info(file_descriptor, path),
            values: enum_descriptor
                .value
                .iter()
                .map(|v| v.name().to_string())
                .collect(),
            namespace,
        }
    }

    fn href_id(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.namespace.join("."), self.name)
        }
    }
}

#[derive(Template)]
#[template(path = "method.html")]
struct Method {
    name: String,
    request_message: SymbolLink,
    response_message: SymbolLink,
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
struct ProtoFileDescriptorTemplate {
    services: Vec<Service>,
    messages: Vec<ProtoMessage>,
    enums: Vec<Enum>,
    filename: String,
}

impl ProtoFileDescriptorTemplate {
    fn from_descriptor(descriptor: FileDescriptorProto, packages: &HashSet<String>) -> Self {
        let namespace = vec![];

        let services = descriptor
            .service
            .iter()
            .enumerate()
            .map(|(service_idx, s)| Service {
                name: s.name().into(),
                methods: s
                    .method
                    .iter()
                    .enumerate()
                    .map(|(method_idx, m)| Method {
                        name: m.name().parse().unwrap(),
                        request_message: SymbolLink::from_type_name(
                            m.input_type.clone().unwrap(),
                            packages,
                        ),
                        response_message: SymbolLink::from_type_name(
                            m.output_type.clone().unwrap(),
                            packages,
                        ),
                        meta: read_source_code_info(
                            &descriptor,
                            &[
                                SERVICE_TAG,
                                service_idx as i32,
                                SERVICE_METHOD_TAG,
                                method_idx as i32,
                            ],
                        ),
                    })
                    .collect(),
                meta: read_source_code_info(&descriptor, &[SERVICE_TAG, service_idx as i32]),
            })
            .collect();

        let messages = descriptor
            .message_type
            .iter()
            .enumerate()
            .map(|(message_idx, m)| {
                ProtoMessage::from_descriptor(
                    &descriptor,
                    m,
                    &[MESSAGE_TYPE_TAG, message_idx as i32],
                    namespace.clone(),
                    packages,
                )
            })
            .collect();

        let enums = descriptor
            .enum_type
            .iter()
            .enumerate()
            .map(|(enum_idx, e)| {
                Enum::from_descriptor(
                    &descriptor,
                    e,
                    &[ENUM_TYPE_TAG, enum_idx as i32],
                    namespace.clone(),
                )
            })
            .collect();

        Self {
            services,
            messages,
            enums,
            filename: descriptor.name().into(),
        }
    }
}

#[derive(Template, Default)]
#[template(path = "namespace.html")]
struct ProtoNamespaceTemplate {
    files: Vec<ProtoFileDescriptorTemplate>,
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
        info.location
            .iter()
            .find(|location| location.path == path)
            .cloned()
    } else {
        None
    }
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
