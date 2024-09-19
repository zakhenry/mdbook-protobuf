use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::Into;
use std::fs::canonicalize;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Error, Result};
use anyhow::anyhow;
use askama::filters::format;
use askama::Template;
use bytes::Bytes;
use clap::arg;
use log::{debug, info, warn};
use mdbook::book::{Book, Chapter, SectionNumber};
use mdbook::BookItem;
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use prost::Message;
use prost_types::{
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    FileDescriptorSet, ServiceDescriptorProto,
};
use prost_types::field_descriptor_proto::Type;
use prost_types::source_code_info::Location;

mod view;
mod primitive;

use view::{ProtoFileDescriptorTemplate, ProtoNamespaceTemplate};
use crate::view::Symbol;
use crate::view::SymbolLink;

pub fn read_file_descriptor_set(path: &Path) -> Result<FileDescriptorSet> {
    let mut file = File::open(path)?;

    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer)?;

    let bytes = Bytes::from(buffer);

    Ok(FileDescriptorSet::decode(bytes)?)
}

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
        let config = ctx.config.get_preprocessor(PREPROCESSOR_NAME).ok_or(anyhow!("Expected config"))?;

        let file_descriptor_path = config.get("proto_descriptor").ok_or(anyhow!("expected `proto_descriptor` key in config"))?;

        let mut path = ctx.root.clone();
        path.push(
            file_descriptor_path.as_str().ok_or(anyhow!("`proto_descriptor` should be a string"))?,
        );

        let file_descriptor_path = canonicalize(path)?;

        Ok(Self {
            file_descriptor_path,
            nest_under: config.get("nest_under").and_then(|v| v.as_str().map(|s| s.to_string())),
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

        let mut symbol_usages: HashMap<SymbolLink, Vec<SymbolLink>> = HashMap::new();

        let packages: HashSet<String> = file_descriptor_set.file.iter().map(|f| f.package().to_string()).collect();

        for file_descriptor in file_descriptor_set.file {
            let value = namespaces.entry(file_descriptor.package().to_string()).or_default();

            value.add_file(ProtoFileDescriptorTemplate::from_descriptor(
                file_descriptor,
                &packages,
                &mut symbol_usages
            ));
        }

        assign_backlinks(&mut namespaces, symbol_usages);

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

        let chapters: Result<Vec<Chapter>> = namespaces.iter().map(|(namespace_key, namespace)| {
            let content = namespace.render()?;
            let path = PathBuf::from(format!("proto/{}", &namespace_key.replace(".", "/")));
            Ok(Chapter::new(
                namespace_key.as_ref(),
                content,
                path,
                Vec::new(),
            ))
        }).collect();

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
            book.sections.extend(chapters?.into_iter().map(BookItem::Chapter));
        }

        Ok(book)
    }

    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer != "not-supported"
    }
}

fn assign_backlinks(document: &mut BTreeMap<String, ProtoNamespaceTemplate>, symbol_usages: HashMap<SymbolLink, Vec<SymbolLink>>) {
    for (_, namespace) in document {

        namespace.mutate_symbols(|symbol| {
            match symbol {
                Symbol::Enum(enum_symbol) => {}
                Symbol::Message(message_symbol) => {
                    if let Some(usages) = symbol_usages.get(&message_symbol.self_link) {
                        message_symbol.set_backlinks(usages.clone())
                    }
                }
            }
        })

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
