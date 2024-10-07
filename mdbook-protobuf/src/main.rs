use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::{fs, io};

use clap::{Arg, ArgMatches, Command};
use log::{debug, error, info};
use mdbook::errors::Error;
use mdbook::preprocess::{CmdPreprocessor, Preprocessor};
use mdbook_protobuf::ProtobufPreprocessor;
use semver::{Version, VersionReq};
use toml_edit::{value, Array, DocumentMut, Item, Table, Value};

const CSS: &[u8] = include_bytes!("assets/mdbook-protobuf.css");
const FILES: &[(&str, &[u8])] = &[("mdbook-protobuf.css", CSS)];

pub fn make_app() -> Command {
    Command::new("nop-preprocessor").about("A mdbook preprocessor which does precisely nothing").subcommand(
        Command::new("supports").arg(Arg::new("renderer").required(true)).about("Check whether a renderer is supported by this preprocessor"),
    ).subcommand(
        Command::new("install").arg(
            Arg::new("dir").default_value(".").help("Root directory for the book,\nshould contain the configuration file (`book.toml`)")
        ).about("Install the required asset files and include it in the config"),
    )
}

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let matches = make_app().get_matches();

    // Users will want to construct their own preprocessor here
    let preprocessor = ProtobufPreprocessor::new();

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        handle_supports(&preprocessor, sub_args);
    } else if let Some(sub_args) = matches.subcommand_matches("install") {
        handle_install(sub_args);
    } else if let Err(e) = handle_preprocessing(&preprocessor) {
        error!("{:?}", e);
        process::exit(1);
    }
}

fn handle_preprocessing(pre: &dyn Preprocessor) -> Result<(), Error> {
    info!("Running mdbook-protobuf");
    let (ctx, book) = CmdPreprocessor::parse_input(io::stdin())?;

    let book_version = Version::parse(&ctx.mdbook_version)?;
    let version_req = VersionReq::parse(mdbook::MDBOOK_VERSION)?;

    if !version_req.matches(&book_version) {
        eprintln!(
            "Warning: The {} plugin was built against version {} of mdbook, \
             but we're being called from version {}",
            pre.name(),
            mdbook::MDBOOK_VERSION,
            ctx.mdbook_version
        );
    }

    let processed_book = pre.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;

    Ok(())
}

fn handle_supports(pre: &dyn Preprocessor, sub_args: &ArgMatches) -> ! {
    let renderer = sub_args
        .get_one::<String>("renderer")
        .expect("Required argument");
    let supported = pre.supports_renderer(renderer);

    // Signal whether the renderer is supported by exiting with 1 or 0.
    if supported {
        process::exit(0);
    } else {
        process::exit(1);
    }
}

fn handle_install(sub_args: &ArgMatches) -> ! {
    let proj_dir = sub_args
        .get_one::<String>("dir")
        .expect("Required argument");
    let proj_dir = PathBuf::from(proj_dir);
    let config = proj_dir.join("book.toml");

    if !config.exists() {
        error!("Configuration file '{}' missing", config.display());
        process::exit(1);
    }

    info!("Reading configuration file {}", config.display());
    let toml = fs::read_to_string(&config).expect("can't read configuration file");
    let mut doc = toml
        .parse::<DocumentMut>()
        .expect("configuration is not valid TOML");

    let has_pre = doc
        .get("preprocessor")
        .and_then(|p| p.get("protobuf"))
        .map(|m| matches!(m, Item::Table(_)))
        .unwrap_or(false);

    if !has_pre {
        info!("Adding preprocessor configuration");

        let empty_table = Item::Table(Table::default());

        let item = doc.entry("preprocessor").or_insert(empty_table.clone());
        let item = item
            .as_table_mut()
            .unwrap()
            .entry("protobuf")
            .or_insert(empty_table)
            .as_table_mut()
            .unwrap();
        item["command"] = value("mdbook-protobuf");
        let mut proto_descriptor: Value = "./path/to/your/proto_file_descriptor_set.pb".into();
        proto_descriptor.decor_mut().set_suffix(" # Edit this!");
        item["proto_descriptor"] = value(proto_descriptor);

        let mut nest: Value = "protocol".into();
        nest.decor_mut().set_suffix(" # if you want to have the proto reference placed as a child of a page - set that page name here.");
        item["nest_under"] = value(nest);
        item.get_key_value_mut("nest_under")
            .unwrap()
            .0
            .leaf_decor_mut()
            .set_prefix("#");

        let mut proto_url_root: Value = "https://example.com/path/to/your/proto/directory".into();
        proto_url_root
            .decor_mut()
            .set_suffix(" # remove this if you don't have a source to link to");
        item["proto_url_root"] = value(proto_url_root);
    }

    let added_files = add_additional_files(&mut doc);

    if !has_pre || added_files {
        info!("Saving changed configuration to {}", config.display());
        let toml = doc.to_string();
        let mut file = File::create(config).expect("can't open configuration file for writing.");
        file.write_all(toml.as_bytes())
            .expect("can't write configuration");
    }

    let mut printed = false;
    for (name, content) in FILES {
        let filepath = proj_dir.join(name);
        if filepath.exists() {
            debug!(
                "'{}' already exists (Path: {}). Skipping.",
                name,
                filepath.display()
            );
        } else {
            if !printed {
                printed = true;
                info!(
                    "Writing additional files to project directory at {}",
                    proj_dir.display()
                );
            }
            debug!("Writing content for '{}' into {}", name, filepath.display());
            let mut file = File::create(filepath).expect("can't open file for writing");
            file.write_all(content)
                .expect("can't write content to file");
        }
    }

    info!("mdbook-protobuf successfully installed. \n\nNow configure your book.toml with the location of your proto descriptor. Refer to the docs for more info.");

    process::exit(0);
}

fn has_file(elem: &Option<&mut Array>, file: &str) -> bool {
    match elem {
        Some(elem) => elem.iter().any(|elem| match elem.as_str() {
            None => true,
            Some(s) => s.ends_with(file),
        }),
        None => false,
    }
}

fn add_additional_files(doc: &mut DocumentMut) -> bool {
    let mut changed = false;
    let mut printed = false;

    for (file, _) in FILES {
        let ext = Path::new(file)
            .extension()
            .and_then(|ext| ext.to_str())
            .expect("file should have extension");

        let additional_section = additional(doc, ext);
        if has_file(&additional_section, file) {
            debug!("'{}' already in 'additional-{}'. Skipping", file, ext)
        } else {
            printed = true;
            info!("Adding additional files to configuration");
            debug!("Adding '{}' to 'additional-{}'", file, ext);
            insert_additional(doc, ext, file);
            changed = true;
        }
    }

    changed
}

fn additional<'a>(doc: &'a mut DocumentMut, additional_type: &str) -> Option<&'a mut Array> {
    let doc = doc.as_table_mut();

    let item = doc.get_mut("output")?;
    let item = item.as_table_mut()?.get_mut("html")?;
    let item = item
        .as_table_mut()?
        .get_mut(&format!("additional-{}", additional_type))?;
    item.as_array_mut()
}

fn insert_additional(doc: &mut DocumentMut, additional_type: &str, file: &str) {
    let doc = doc.as_table_mut();

    let empty_table = Item::Table(Table::default());
    let empty_array = Item::Value(Value::Array(Array::default()));
    let item = doc.entry("output").or_insert(empty_table.clone());
    let item = item
        .as_table_mut()
        .unwrap()
        .entry("html")
        .or_insert(empty_table);
    let array = item
        .as_table_mut()
        .unwrap()
        .entry(&format!("additional-{}", additional_type))
        .or_insert(empty_array);
    array
        .as_value_mut()
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(file);
}
