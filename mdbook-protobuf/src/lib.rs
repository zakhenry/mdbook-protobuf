use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Error, Result};
use bytes::Bytes;
use prost::Message;
use prost_types::{FileDescriptorProto, FileDescriptorSet, ServiceDescriptorProto};

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

struct ProtoMessage {
    name: String,
    namespace: String,
}

#[derive(Template)]
#[template(path = "method.html")]
struct Method {
    name: String,
    request_message: ProtoMessage,
    response_message: ProtoMessage,
}

#[derive(Template)]
#[template(path = "service.html")]
struct Service<'a> {
    name: &'a str,
    methods: Vec<Method>
}

#[derive(Template)]
#[template(path = "proto.html")]
struct ProtoFileTemplate<'a> {
    services: Vec<Service<'a>>
}

fn render_protobuf_descriptor(descriptor: FileDescriptorProto) -> Result<String> {

    let services = descriptor.service.iter().map(|s| Service {
        name: s.name(),
        methods: s.method.iter().map(|m|Method {
            name: m.name().parse().unwrap(),
            request_message: ProtoMessage { name: m.clone().input_type.unwrap(), namespace: "".into() },
            response_message: ProtoMessage { name: m.clone().output_type.unwrap(), namespace: "".into() },
        }).collect()
    }).collect();

    let view = ProtoFileTemplate { services };

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

