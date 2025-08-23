fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure tonic-build for production protobuf compilation
    let out_dir = std::env::var("OUT_DIR").unwrap();
    
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .extern_path(".google.protobuf.Duration", "::prost_types::Duration")
        .file_descriptor_set_path(format!("{}/ratelimit_descriptor.bin", out_dir))
        .compile(
            &[
                "proto/ratelimit.proto",
                "proto/config.proto",
            ],
            &["proto"],
        )?;
    
    // Include the generated protobuf files in the crate
    println!("cargo:rerun-if-changed=proto/");
    println!("cargo:rerun-if-changed=build.rs");
    
    // Tell Cargo to include the generated files
    println!("cargo:rustc-env=PROTO_OUT_DIR={}", out_dir);
    
    Ok(())
}