use std::{env, fs};
use std::fs::File;
use std::io::Read;
use std::path::Path;

// fn copy_files(from:&str, to: &str) -> Result<(), Box<dyn std::error::Error>> {
//     let out_dir = env::var("OUT_DIR")?;
//     let src_path = Path::new(&out_dir).join(from);
//     let dest_path = Path::new(to);
//     let mut src_file = File::open(src_path)?;
//     let mut contents = String::new();
//     src_file.read_to_string(&mut contents)?;
//     fs::write(&dest_path, contents)?;
// }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(yes) = env::var("GENERATE_PROTOBUF") {
        if yes == "Y" {
            tonic_build::configure()
                .build_server(true)
                .out_dir("src/pb")
                .type_attribute(".","#[derive(serde::Serialize, serde::Deserialize)]") // you can change the generated code's location
                .compile(
                    &["proto/mpc.proto", "proto/types.proto"],
                    &["proto"], // specify the root location to search proto dependencies
                )?;
        }
    }
    Ok(())
}
// use prost_build;
// use std::io::Result;
// use env;
// use fs;
// fn main() -> Result<()> {
//     prost_build::compile_protos(&["proto/mpc.proto"], &["proto"])?;
//     // let out_dir = env::var_os("OUT_DIR").unwrap();
//     // let dest_path = Path::new(&out_dir).join("mpc.rs");
//     // fs::write(&dest_path, code).unwrap();
//     Ok(())
// }