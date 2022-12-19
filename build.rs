fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/mpc.proto")?;
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