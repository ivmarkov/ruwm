use std::iter;
use std::{env, path::PathBuf};

use embuild::utils::OsStrExt;
use embuild::*;

fn main() -> anyhow::Result<()> {
    build_ulp()?;

    build::CfgArgs::output_propagated("ESP_IDF")?;
    build::LinkArgs::output_propagated("ESP_IDF")?;

    Ok(())
}

fn build_ulp() -> anyhow::Result<()> {
    let source = path_buf![env::current_dir()?, "src", "pulse_counter.S"];
    cargo::track_file(&source);

    let build_result = espidf::ulp_fsm::Builder::try_from_embuild_env("ESP_IDF", vec![])?.build(
        iter::once(source.as_path()),
        PathBuf::from(env::var("OUT_DIR")?).join("ulp_fsm"),
    )?;

    cargo::set_rustc_env("ULP_FSM_BIN", build_result.bin_file.try_to_str()?);
    cargo::set_rustc_env("ULP_FSM_RS", build_result.sym_rs_file.try_to_str()?);

    Ok(())
}
