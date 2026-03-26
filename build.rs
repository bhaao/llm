//! Build script for compiling Protobuf files
//! Only runs when the `grpc` feature is enabled

#[cfg(feature = "grpc")]
fn compile_protobuf() {
    println!("cargo:rerun-if-changed=proto/");

    // 检查 proto 文件是否存在
    let proto_files = vec!["proto/node_rpc.proto", "proto/consensus.proto"];

    for proto_file in &proto_files {
        if !std::path::Path::new(proto_file).exists() {
            eprintln!("\n❌ ERROR: Proto file {} not found", proto_file);
            eprintln!("   gRPC feature requires protobuf definition files.\n");
            std::process::exit(1);
        }
    }

    // 检查 protoc 是否可用
    match std::process::Command::new("protoc")
        .arg("--version")
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!("cargo:warning=protoc detected: {}", version);
            } else {
                print_installation_guide();
                std::process::exit(1);
            }
        }
        Err(_) => {
            print_installation_guide();
            std::process::exit(1);
        }
    }

    // 编译 protobuf 文件到 OUT_DIR
    // 生成的文件将包含在 env!("OUT_DIR")/block_chain_with_context.rs
    match tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&proto_files, &["proto"])
    {
        Ok(_) => {
            println!("cargo:note=gRPC protobuf files compiled successfully");
        }
        Err(e) => {
            eprintln!("\n❌ ERROR: Failed to compile protobuf: {}", e);
            eprintln!("   Please check proto files and protoc version compatibility.\n");
            std::process::exit(1);
        }
    }
}

#[cfg(feature = "grpc")]
fn print_installation_guide() {
    eprintln!("\n❌ ERROR: protoc (protobuf compiler) not found");
    eprintln!("   gRPC feature requires protobuf-compiler to build.\n");
    eprintln!("   Installation guide:");
    eprintln!("     - Debian/Ubuntu: apt-get install protobuf-compiler");
    eprintln!("     - macOS:         brew install protobuf");
    eprintln!("     - Arch Linux:    pacman -S protobuf");
    eprintln!("     - Windows:       Download from https://github.com/protocolbuffers/protobuf/releases");
    eprintln!("\n   Or disable gRPC feature:");
    eprintln!("     cargo build --no-default-features --features rpc\n");
}

#[cfg(not(feature = "grpc"))]
fn compile_protobuf() {
    // gRPC 特性未启用，跳过 protobuf 编译
}

fn main() {
    compile_protobuf();
}
