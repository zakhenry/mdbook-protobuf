fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .build_server(true)
        .file_descriptor_set_path("./docs/build/proto_file_descriptor_set.pb")
        .compile(
            &[
                "./proto/helloworld.proto",
                "./proto/type_test.proto",
                "./proto/type_test_child_package.proto",
                "./proto/type_test_same_namespace.proto",
            ],
            &["./proto"],
        )?;

    Ok(())
}
