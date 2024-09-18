use prost_types::field_descriptor_proto::Type;
use askama::Template;
use prost_types::source_code_info::Location;
use prost_types::{DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto};
use std::collections::HashSet;

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
        }.into();

        let best_match = packages.iter().filter(|value| type_name[1..].starts_with(value.as_str())).max_by_key(|value| value.len());

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

pub(crate) enum FieldType {
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
            nested_message: message_descriptor.nested_type.iter().enumerate().map(|(idx, m)| {
                let mut nested_path = source_path.to_vec();
                nested_path.extend(&[idx as i32]);
                ProtoMessage::from_descriptor(
                    file_descriptor,
                    m,
                    nested_path.as_ref(),
                    nested_namespace.clone(),
                    packages,
                )
            }).collect(),
            nested_enum: message_descriptor.enum_type.iter().enumerate().map(|(idx, m)| {
                let mut nested_path = source_path.to_vec();
                nested_path.extend(&[idx as i32]);
                Enum::from_descriptor(
                    file_descriptor,
                    m,
                    nested_path.as_ref(),
                    nested_namespace.clone(),
                )
            }).collect(),
            fields: message_descriptor.field.iter().enumerate().map(|(idx, f)| {
                let mut nested_path = source_path.to_vec();
                nested_path.extend(&[SERVICE_METHOD_TAG, idx as i32]);
                Field::from_descriptor(file_descriptor, f, nested_path.as_ref(), packages)
            }).collect(),
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
            values: enum_descriptor.value.iter().map(|v| v.name().to_string()).collect(),
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
pub struct ProtoFileDescriptorTemplate {
    services: Vec<Service>,
    messages: Vec<ProtoMessage>,
    enums: Vec<Enum>,
    filename: String,
}

impl ProtoFileDescriptorTemplate {
    pub(crate) fn from_descriptor(descriptor: FileDescriptorProto, packages: &HashSet<String>) -> Self {
        let namespace = vec![];

        let services = descriptor.service.iter().enumerate().map(|(service_idx, s)| Service {
            name: s.name().into(),
            methods: s.method.iter().enumerate().map(|(method_idx, m)| Method {
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
            }).collect(),
            meta: read_source_code_info(&descriptor, &[SERVICE_TAG, service_idx as i32]),
        }).collect();

        let messages = descriptor.message_type.iter().enumerate().map(|(message_idx, m)| {
            ProtoMessage::from_descriptor(
                &descriptor,
                m,
                &[MESSAGE_TYPE_TAG, message_idx as i32],
                namespace.clone(),
                packages,
            )
        }).collect();

        let enums = descriptor.enum_type.iter().enumerate().map(|(enum_idx, e)| {
            Enum::from_descriptor(
                &descriptor,
                e,
                &[ENUM_TYPE_TAG, enum_idx as i32],
                namespace.clone(),
            )
        }).collect();

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
pub struct ProtoNamespaceTemplate {
    files: Vec<ProtoFileDescriptorTemplate>,
}

impl ProtoNamespaceTemplate {
    pub(crate) fn add_file(&mut self, file: ProtoFileDescriptorTemplate) {
        self.files.push(file);
    }
}

// these tags come from FileDescriptorProto - prost doesn't provide a way to read this as-yet
// see https://github.com/tokio-rs/prost/issues/137 const SERVICE_METHOD_TAG: i32 = 2; const DESCRIPTOR_FIELD_TAG: i32 = 2;
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
