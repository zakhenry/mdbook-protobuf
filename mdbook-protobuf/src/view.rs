use crate::links::{Backlinks, Linked, SymbolLink};
use askama::Template;
use prost_types::field_descriptor_proto::Type;
use prost_types::source_code_info::Location;
use prost_types::{
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    OneofDescriptorProto,
};
use std::collections::{HashMap, HashSet};

pub(crate) enum FieldType {
    Symbol(SymbolLink),
    Primitive(Type),
    Unimplemented,
}

#[derive(Template)]
#[template(path = "field.html")]
struct SimpleField {
    name: String,
    meta: Option<Location>,
    typ: FieldType,
    optional: bool,
    oneof_index: Option<i32>,
    deprecated: bool,
    self_link: SymbolLink,
}

impl SimpleField {
    fn from_descriptor(
        file_descriptor: &FileDescriptorProto,
        field_descriptor: &FieldDescriptorProto,
        path: &[i32],
        packages: &HashSet<String>,
        parent_symbol: &SymbolLink,
    ) -> Self {
        let name: String = field_descriptor.name().into();
        let mut self_link = parent_symbol.clone();
        self_link.set_property(name.clone());

        Self {
            name,
            meta: read_source_code_info(file_descriptor, path),
            typ: match field_descriptor.r#type {
                None => {
                    FieldType::Unimplemented // todo look up fully qualified from index.
                }
                Some(label) => match Type::try_from(label).expect("should be of type") {
                    Type::Enum | Type::Message => FieldType::Symbol(SymbolLink::from_fqsl(
                        field_descriptor.type_name().to_string(),
                        packages,
                    )),
                    t => FieldType::Primitive(t),
                },
            },
            optional: field_descriptor.proto3_optional.unwrap_or(false),
            oneof_index: field_descriptor.oneof_index,
            deprecated: field_descriptor
                .clone()
                .options
                .map_or(false, |o| o.deprecated()),
            self_link,
        }
    }
}

#[derive(Template)]
#[template(path = "oneof_field.html")]
struct OneOfField {
    name: String,
    meta: Option<Location>,
    fields: Vec<SimpleField>,
}

impl OneOfField {
    fn from_descriptor(
        file_descriptor: &FileDescriptorProto,
        oneof_descriptor: &OneofDescriptorProto,
        path: &[i32],
    ) -> Self {
        Self {
            name: oneof_descriptor.name().into(),
            meta: read_source_code_info(file_descriptor, path),
            fields: Vec::new(),
        }
    }
}

enum Field {
    Simple(SimpleField),
    OneOf(OneOfField),
}

#[derive(Template)]
#[template(path = "message.html")]
pub(crate) struct ProtoMessage {
    name: String,
    meta: Option<Location>,
    nested_message: Vec<ProtoMessage>,
    nested_enum: Vec<Enum>,
    fields: Vec<Field>,
    namespace: Vec<String>,
    deprecated: bool,
    self_link: SymbolLink,
    backlinks: Backlinks,
}

impl ProtoMessage {
    fn from_descriptor(
        file_descriptor: &FileDescriptorProto,
        message_descriptor: &DescriptorProto,
        source_path: &[i32],
        parent_messages: Vec<String>,
        packages: &HashSet<String>,
        package: String,
        symbol_usages: &mut HashMap<SymbolLink, Vec<SymbolLink>>,
    ) -> Self {
        let name: String = message_descriptor.name().into();
        let mut message_path = parent_messages.clone();
        message_path.push(message_descriptor.name().into());

        let fqsl = format!(".{}.{}", package, message_path.join("."));
        let self_link = SymbolLink::from_fqsl(fqsl, packages);

        let all_fields: Vec<SimpleField> = message_descriptor
            .field
            .iter()
            .enumerate()
            .map(|(idx, f)| {
                let mut nested_path = source_path.to_vec();
                nested_path.extend(&[MESSAGE_FIELD_TAG, idx as i32]);

                SimpleField::from_descriptor(
                    file_descriptor,
                    f,
                    nested_path.as_ref(),
                    packages,
                    &self_link,
                )
            })
            .collect();

        let mut oneofs: HashMap<i32, OneOfField> = message_descriptor
            .oneof_decl
            .iter()
            .enumerate()
            .map(|(idx, o)| {
                let mut nested_path = source_path.to_vec();
                nested_path.extend(&[MESSAGE_ONEOF_TAG, idx as i32]);
                (
                    idx as i32,
                    OneOfField::from_descriptor(file_descriptor, o, nested_path.as_ref()),
                )
            })
            .collect();

        let mut fields = Vec::new();

        for field in all_fields {
            if let FieldType::Symbol(symbol_link) = &field.typ {
                let mut field_ref = self_link.clone();
                field_ref.set_property(field.name.clone());

                symbol_usages
                    .entry(symbol_link.clone())
                    .or_default()
                    .push(field_ref.clone());
            }

            if let Some(oneof_index) = field.oneof_index {
                oneofs
                    .get_mut(&oneof_index)
                    .expect("field should exist")
                    .fields
                    .push(field)
            } else {
                fields.push(Field::Simple(field));
            }
        }

        fields.extend(oneofs.into_values().into_iter().map(Field::OneOf));

        Self {
            name,
            self_link,
            namespace: parent_messages,
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
                        message_path.clone(),
                        packages,
                        package.clone(),
                        symbol_usages,
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
                        packages,
                        package.clone(),
                        message_path.clone(),
                    )
                })
                .collect(),
            fields,
            deprecated: message_descriptor
                .options
                .clone()
                .map_or(false, |o| o.deprecated()),
            backlinks: Default::default(),
        }
    }
}

impl Linked for ProtoMessage {
    fn symbol_link(&self) -> &SymbolLink {
        &self.self_link
    }

    fn set_backlinks(&mut self, backlinks: Backlinks) {
        self.backlinks = backlinks
    }
}

struct EnumValue {
    tag: i32,
    name: String,
    deprecated: bool,
}

#[derive(Template)]
#[template(path = "enum.html")]
pub(crate) struct Enum {
    name: String,
    meta: Option<Location>,
    values: Vec<EnumValue>,
    namespace: Vec<String>,
    backlinks: Backlinks,
    self_link: SymbolLink,
}

impl Enum {
    fn from_descriptor(
        file_descriptor: &FileDescriptorProto,
        enum_descriptor: &EnumDescriptorProto,
        path: &[i32],
        packages: &HashSet<String>,
        package: String,
        namespace: Vec<String>,
    ) -> Self {
        let name: String = enum_descriptor.name().into();

        let mut fq = namespace.clone();
        fq.push(name.clone());
        let fqsl = format!(".{}.{}", package, fq.join("."));

        Self {
            name,
            meta: read_source_code_info(file_descriptor, path),
            values: enum_descriptor
                .value
                .iter()
                .map(|v| EnumValue {
                    name: v.name().to_string(),
                    tag: v.number(),
                    deprecated: v.clone().options.map_or(false, |o| o.deprecated()),
                })
                .collect(),
            namespace,
            backlinks: Default::default(),
            self_link: SymbolLink::from_fqsl(fqsl, packages),
        }
    }
}

impl Linked for Enum {
    fn symbol_link(&self) -> &SymbolLink {
        &self.self_link
    }

    fn set_backlinks(&mut self, backlinks: Backlinks) {
        self.backlinks = backlinks
    }
}

#[derive(Template)]
#[template(path = "method.html")]
struct Method {
    name: String,
    request_message: SymbolLink,
    response_message: SymbolLink,
    meta: Option<Location>,
    deprecated: bool,
    self_link: SymbolLink,
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
    pub(crate) fn from_descriptor(
        descriptor: FileDescriptorProto,
        packages: &HashSet<String>,
        symbol_usages: &mut HashMap<SymbolLink, Vec<SymbolLink>>,
    ) -> Self {
        let parent_messages = vec![];

        let services = descriptor
            .service
            .iter()
            .enumerate()
            .map(|(service_idx, s)| {
                let service_name: String = s.name().into();
                Service {
                    name: service_name.clone(),
                    methods: s
                        .method
                        .iter()
                        .enumerate()
                        .map(|(method_idx, m)| {
                            let method_name: String = m.name().parse().unwrap();

                            let method_link = SymbolLink::from_fqsl(
                                format!(
                                    ".{}.{}::{}",
                                    descriptor.package(),
                                    &service_name,
                                    &method_name
                                ),
                                packages,
                            );
                            symbol_usages.entry(method_link.clone()).or_default();

                            let request_message =
                                SymbolLink::from_fqsl(m.input_type.clone().unwrap(), packages);

                            symbol_usages
                                .entry(request_message.clone())
                                .or_default()
                                .push(method_link.clone());

                            let response_message =
                                SymbolLink::from_fqsl(m.output_type.clone().unwrap(), packages);

                            symbol_usages
                                .entry(response_message.clone())
                                .or_default()
                                .push(method_link.clone());

                            Method {
                                name: method_name,
                                request_message,
                                response_message,
                                self_link: method_link,
                                meta: read_source_code_info(
                                    &descriptor,
                                    &[
                                        SERVICE_TAG,
                                        service_idx as i32,
                                        SERVICE_METHOD_TAG,
                                        method_idx as i32,
                                    ],
                                ),
                                deprecated: m.options.clone().map_or(false, |o| o.deprecated()),
                            }
                        })
                        .collect(),
                    meta: read_source_code_info(&descriptor, &[SERVICE_TAG, service_idx as i32]),
                }
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
                    parent_messages.clone(),
                    packages,
                    descriptor.package().to_string(),
                    symbol_usages,
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
                    packages,
                    descriptor.package().to_string(),
                    parent_messages.clone(),
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
pub struct ProtoNamespaceTemplate {
    files: Vec<ProtoFileDescriptorTemplate>,
}

impl ProtoNamespaceTemplate {
    pub(crate) fn add_file(&mut self, file: ProtoFileDescriptorTemplate) {
        self.files.push(file);
    }

    pub(crate) fn mutate_messages<F>(messages: &mut Vec<ProtoMessage>, mut mutator: F)
    where
        F: Fn(&mut dyn Linked) + Clone,
    {
        for message in messages {
            mutator(message);
            Self::mutate_messages(&mut message.nested_message, mutator.clone());
        }
    }

    pub(crate) fn mutate_symbols<F>(&mut self, mut mutator: F)
    where
        F: Fn(&mut dyn Linked) + Clone,
    {
        for mut file in &mut self.files {
            Self::mutate_messages(&mut file.messages, mutator.clone());

            for enum_type in &mut file.enums {
                mutator(enum_type)
            }
        }
    }
}

// these tags come from FileDescriptorProto - prost doesn't provide a way to read this as-yet
// see https://github.com/tokio-rs/prost/issues/137 const SERVICE_METHOD_TAG: i32 = 2; const DESCRIPTOR_FIELD_TAG: i32 = 2;
const SERVICE_METHOD_TAG: i32 = 2;
const MESSAGE_FIELD_TAG: i32 = 2;
const MESSAGE_ONEOF_TAG: i32 = 8;
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
