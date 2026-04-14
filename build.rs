fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    let proto_dir = "proto";

    // Database / VMM protos — client stubs (we call out to these services).
    let db_protos = [
        "proto/v1/lancedb.proto",
        "proto/v1/sqlite.proto",
        "proto/v1/vmm.proto",
    ];
    for p in &db_protos {
        println!("cargo:rerun-if-changed={}", p);
    }

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(false)
        .compile_protos(&db_protos, &[proto_dir])?;

    // MCP service proto — server only.  We do NOT need a client stub for this
    // service (clients connect *to* us).  Skipping client generation also avoids
    // a name collision: the generated channel `connect()` helper would clash with
    // the RPC `Connect` method in tonic-prost-build 0.14.
    let mcp_protos = ["proto/v1/mcp_service.proto"];
    println!("cargo:rerun-if-changed=proto/v1/mcp_service.proto");

    tonic_prost_build::configure()
        .build_client(false)
        .build_server(true)
        .compile_protos(&mcp_protos, &[proto_dir])?;

    Ok(())
}
