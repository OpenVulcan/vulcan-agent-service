/// Generate protobuf bindings and configure binary-specific linker behavior.
/// 生成 Protobuf 绑定并配置二进制专属链接行为。
/// Returns success after code generation and linker directive emission, or the underlying build error.
/// 代码生成及链接指令输出完成后返回成功，否则返回底层构建错误。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    let proto_dir = "proto";

    // VMM proto — client and server stubs for backend relay mode.
    let vmm_protos = ["proto/v1/vmm.proto"];
    for p in &vmm_protos {
        println!("cargo:rerun-if-changed={}", p);
    }

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&vmm_protos, &[proto_dir])?;

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

    // LuaJIT static objects carry CRT export directives; this executable has no consumers for the auxiliary import/export libraries they would generate.
    // LuaJIT 静态对象携带 CRT 导出指令；本可执行文件没有辅助导入库与导出库的消费者，因此无需生成这些文件。
    // Apply MSVC-only switches according to the compilation target, including cross builds.
    // 根据编译目标应用 MSVC 专用开关，交叉编译时同样遵循目标平台。
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=vulcan-agent-service=/NOEXP");
        println!("cargo:rustc-link-arg-bin=vulcan-agent-service=/NOIMPLIB");
    }

    // Native Lua modules resolve LuaJIT C symbols from the running executable.
    // 原生 Lua 模块需要从当前可执行文件解析 LuaJIT C 符号。
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {
            println!("cargo:rustc-link-arg-bin=vulcan-agent-service=-Wl,--export-dynamic")
        }
        Ok("macos") => {
            println!("cargo:rustc-link-arg-bin=vulcan-agent-service=-Wl,-export_dynamic");
            // Packaged Lua modules reference @rpath libraries kept beside the runtime.
            // 已打包的 Lua 模块通过 @rpath 引用运行目录内的动态库。
            println!(
                "cargo:rustc-link-arg-bin=vulcan-agent-service=-Wl,-rpath,@executable_path/../lua_runtime/libs"
            );
        }
        _ => {}
    }

    Ok(())
}
