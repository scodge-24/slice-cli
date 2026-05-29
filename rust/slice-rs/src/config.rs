use serde::Deserialize;

use crate::context::Context;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Ambiguity {
    Strict,
    BestEffort,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    context: Option<RawContext>,
}

#[derive(Debug, Deserialize)]
struct RawContext {
    ambiguity: Option<String>,
}

pub fn load_ambiguity(ctx: &Context) -> Result<Ambiguity> {
    let path = ctx.slices_dir().join("config.yaml");
    if !path.exists() {
        return Ok(Ambiguity::Strict);
    }

    let raw_text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    let parsed: RawConfig = yaml_serde::from_str(&raw_text).map_err(|source| Error::Yaml {
        path: ctx.rel(&path),
        source,
    })?;
    let Some(context_config) = parsed.context else {
        return Ok(Ambiguity::Strict);
    };
    let raw = context_config
        .ambiguity
        .unwrap_or_else(|| "strict".to_owned())
        .trim()
        .to_owned();
    if raw.is_empty() || raw == "strict" {
        Ok(Ambiguity::Strict)
    } else if raw == "best_effort" {
        Ok(Ambiguity::BestEffort)
    } else {
        Err(Error::InvalidInput(format!(
            "invalid context.ambiguity '{raw}' in {}; allowed: strict, best_effort",
            ctx.rel(&path)
        )))
    }
}
