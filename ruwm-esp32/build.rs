use embuild::*;

fn main() -> anyhow::Result<()> {
    let cfg = build::CfgArgs::try_from_env("ESP_IDF")?;

    #[cfg(feature = "ulp")]
    {
        if cfg.get("esp32").is_some() || cfg.get("esp32s2").is_some() {
            build_ulp()?;
        }
    }

    cfg.output();
    build::LinkArgs::output_propagated("ESP_IDF")?;

    edge_frame::assets::prepare::run(
        "RUWM_WEB",
        path_buf![std::env::current_dir()?, "..", "ruwm-web", "dist"],
    )?;

    Ok(())
}

#[cfg(feature = "ulp")]
fn build_ulp() -> anyhow::Result<()> {
    use std::iter;
    use std::{env, path::PathBuf};

    use embuild::utils::OsStrExt;

    let source = path_buf![env::current_dir()?, "src", "ulp_pulse_counter.S"];
    cargo::track_file(&source);

    let build_result = espidf::ulp_fsm::Builder::try_from_embuild_env("ESP_IDF", vec![])?.build(
        iter::once(source.as_path()),
        PathBuf::from(env::var("OUT_DIR")?).join("ulp_fsm"),
    )?;

    cargo::set_rustc_env("ULP_FSM_BIN", build_result.bin_file.try_to_str()?);
    cargo::set_rustc_env("ULP_FSM_RS", build_result.sym_rs_file.try_to_str()?);

    Ok(())
}
