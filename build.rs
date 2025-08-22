fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure tonic-build for production protobuf compilation
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .extern_path(".google.protobuf.Duration", "::prost_types::Duration")
        .compile(
            &[
                "proto/envoy/service/ratelimit/v3/rls.proto",
            ],
            &["proto"],
        )?;
    
    println!("cargo:rerun-if-changed=proto/");
    println!("cargo:rerun-if-changed=build.rs");
    
    Ok(())
}