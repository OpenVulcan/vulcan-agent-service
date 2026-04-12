fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    let proto_dir = "proto";
    let proto_files = [
        "proto/v1/lancedb.proto",
        "proto/v1/sqlite.proto",
        "proto/v1/vmm.proto",
    ];

    for proto in &proto_files {
        println!("cargo:rerun-if-changed={}", proto);
    }

    tonic_prost_build::configure()
        .build_client(true)
        .compile_protos(&proto_files, &[proto_dir])?;

    Ok(())
}
