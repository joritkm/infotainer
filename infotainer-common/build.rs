use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../proto/infotainer.proto";

    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional") // for older systems
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(PathBuf::new().join("infotainer.bin"))
        .out_dir("./src")
        .compile(&[proto_file], &["../proto"])?;

    Ok(())
}
