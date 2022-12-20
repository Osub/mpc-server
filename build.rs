use std::env;

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
