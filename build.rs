// Build script for CrabCache
// Phase 8.1 - Protobuf Native Support

use std::io::Result;
use std::path::Path;

fn main() -> Result<()> {
    // Create output directory if it doesn't exist
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::create_dir_all(&out_dir)?;
    
    // Also create a predictable location for Docker builds
    let src_generated_dir = "src/protocol/protobuf/generated";
    std::fs::create_dir_all(src_generated_dir)?;
    
    // Generate Rust code from protobuf files
    let mut config = prost_build::Config::new();
    config.out_dir(&out_dir);
    
    // Also generate to the source directory for Docker builds
    config.compile_protos(&["proto/crabcache.proto"], &["proto/"])?;
    
    // Copy the generated file to a predictable location
    let generated_file = format!("{}/crabcache.v1.rs", out_dir);
    let target_file = format!("{}/crabcache_v1.rs", src_generated_dir);
    
    if Path::new(&generated_file).exists() {
        std::fs::copy(&generated_file, &target_file)?;
        println!("cargo:warning=Copied generated protobuf to {}", target_file);
    }
    
    // Tell cargo to rerun this build script if the proto file changes
    println!("cargo:rerun-if-changed=proto/crabcache.proto");
    
    Ok(())
}