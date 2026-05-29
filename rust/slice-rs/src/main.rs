#![forbid(unsafe_code)]

fn main() -> anyhow::Result<()> {
    std::process::exit(slice_rs::cli::run()?);
}
