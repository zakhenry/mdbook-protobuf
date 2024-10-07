use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::Into;
use std::fs::canonicalize;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use anyhow::{Error, Result};
use askama::filters::format;
use askama::Template;
use bytes::Bytes;
use clap::arg;
use links::{Backlinks, ProtoSymbol};
use log::{debug, info, warn};
use mdbook::book::{Book, Chapter, SectionNumber};
use mdbook::preprocess::{Preprocessor, PreprocessorContext};
use mdbook::BookItem;
use prost::Message;
use prost_types::field_descriptor_proto::Type;
use prost_types::source_code_info::Location;
use prost_types::{
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    FileDescriptorSet, ServiceDescriptorProto,
};

mod links;
mod primitive;
mod view;

use links::SymbolLink;
use view::{ProtoFileDescriptorTemplate, ProtoNamespaceTemplate};

pub fn read_file_descriptor_set(path: &Path) -> Result<FileDescriptorSet> {
    info!("Attempting to read {}", path.display());

    let mut file = File::open(path).map_err(|e| {
        anyhow!(
            "Could not read file at path `{}`, does it exist here?",
            path.display()
        )
    })?;

    info!("File descriptor set file found at {}", path.display());
    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer)?;

    let bytes = Bytes::from(buffer);

    let decoded = FileDescriptorSet::decode(bytes)
        .map_err(|e| anyhow!("failed to parse file descriptor set as protobuf"))?;

    info!("Successfully decoded file descriptor set");
    Ok(decoded)
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
    proto_url_root: Option<String>,
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

        let file_descriptor_path = canonicalize(path.clone()).map_err(|e| {
            anyhow!(
                "Failed to find `proto_descriptor` at path {}",
                path.display()
            )
        })?;

        Ok(Self {
            file_descriptor_path,
            nest_under: config
                .get("nest_under")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            proto_url_root: config
                .get("proto_url_root")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
        })
    }
}

impl Preprocessor for ProtobufPreprocessor {
    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book, Error> {
        let args = ProtobufPreprocessorArgs::new(ctx)?;

        let file_descriptor_set = read_file_descriptor_set(args.file_descriptor_path.as_path())?;

        info!("found {} proto files", file_descriptor_set.file.len());

        let mut namespaces: BTreeMap<String, ProtoNamespaceTemplate> = BTreeMap::new();

        let mut symbol_usages: HashMap<SymbolLink, Vec<links::Backlink>> = HashMap::new();

        let packages: HashSet<String> = file_descriptor_set
            .file
            .iter()
            .map(|f| f.package().to_string())
            .collect();

        for file_descriptor in file_descriptor_set.file {
            let value = namespaces
                .entry(file_descriptor.package().to_string())
                .or_default();

            value.add_file(ProtoFileDescriptorTemplate::from_descriptor(
                file_descriptor,
                &packages,
                &mut symbol_usages,
            ));
        }

        for book_item in &mut book.sections {
            if let BookItem::Chapter(chapter) = book_item {
                links::link_proto_symbols(chapter, &mut symbol_usages)?;
            }
        }

        links::assign_backlinks(&mut namespaces, symbol_usages);

        if let Some(source_url) = args.proto_url_root {
            info!("assigning source url to proto symbols: {}", &source_url);
            links::assign_source_url(&mut namespaces, source_url);
        } else {
            warn!("proto_url_root was not set, so `[src]` links will not go to the correct destination");
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
                                "proto_descriptor": "../demo/docs/build/proto_file_descriptor_set.pb",
                                "proto_url_root": "http://example.com/proto/"
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
