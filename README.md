# mdbook-protobuf


**mdbook-protobuf** is preprocessor for [`mdbook`](https://rust-lang.github.io/mdBook), allowing developers to generate
reference documentation from `.proto` [protobuf](https://protobuf.dev) definition files.

## Features
* Linking from documentation to messages, fields, enums, services & service methods
* Backlink generation to see from where a symbol is referenced
* Source linking to jump to the `.proto` source code
* Generation from file descriptor set (*not* `.proto` files; this allows you to keep your existing `protoc` invocation
  and just reference the file location)

## Quickstart

### Install
To install it from source:

```shell
cargo install mdbook-protobuf
```

Note currently there is no alternative method, but [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) is
planned, alongside downloadable release artifacts.

### Configure
```shell
mdbook-protobuf install path/to/your/book
```

This will add the following to your `book.toml`, and copy the stylesheets across

```toml
[preprocessor.protobuf]
proto_descriptor = "./path/to/your/proto_file_descriptor_set.pb" # edit this!
#nest_under = "Protocol" # if you want to have the proto reference placed as a child of a page - set that page name here.
proto_url_root = "https://example.com/path/to/your/proto/directory" # remove this if you don't have a source to link to
```

Update the configuration as required, see below for `proto_file_descriptor_set.pb` generation.

### Generating file descriptor set
This file is a more readily machine-readable definition of your `.proto` files. It is already generated as a part of
code generation for your respective language, but it is not normally emitted to disk. **mdbook-protobuf** uses this file
as it takes into account any cli flags or path customisations that you have defined in your protobuf configuration.

If you are using `protoc` directly, simply add the flags
`--descriptor_set_out=proto_file_descriptor_set.pb --include_imports --include_source_info` to your `protoc` command
flags, and reference the generated file in the `book.toml` as described above.

* `--include_imports` is not required, but will allow you to reference external files like google well known types
* `--include_source_info` is not strictly required, but highly recommended as otherwise the generated reference will
  contain no comments or links to source code.

Now that is all set up, rerun
```shell
mdbook serve
```

and you should see the protobuf reference generated in the sidenav. If you don't want the reference all flattened at the
top level, use the `nest_under` config value in `book.toml` to define which page to nest the protobufs under.
This can be a good idea as it allows you to write a preamble to discuss the use of the protocol etc.

*Currently, the `nest_under` page must be a top level page*

### Linking to symbols

Often you will want to reference a particular message or service method in your documentation. **mdbook-protobuf**
enables this with a link macro. For example the following markdown:

```markdown
Here's a link to a protobuf message definition: [HelloRequest](proto!(HelloRequest))
```

will generate the following output:

---
Here's a link to a protobuf message definition: [HelloRequest](proto!(HelloRequest))

---
This symbol lookup is done with the following style:


| type    | markdown                                       | output                                       |
|---------|------------------------------------------------|----------------------------------------------|
| service | `[Greeter service](proto!(Greeter))`           | [Greeter service](proto!(Greeter))           |
| method  | `[Hello stream](proto!(Greeter::StreamHello))` | [Hello stream](proto!(Greeter::StreamHello)) |                                    |
| message | `[Request name](proto!(HelloRequest))`         | [Request](proto!(HelloRequest))              |                                    |
| field   | `[Request name](proto!(HelloRequest::name))`   | [Request name](proto!(HelloRequest::name))   |                                    |
