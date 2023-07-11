use anyhow::{anyhow, bail, Context, Result};
use camino::Utf8Path;
use indicatif::ProgressBar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::fs::{create_dir_all, write};

use crate::{config::SAMOGON_DATA_DIR, ui::fetch_bar_style};

const FORMULAE_URL: &str = "https://formulae.brew.sh/api/formula.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FormulaStable {
    pub name: String,
    pub description: String,
    pub version: String,
    pub revision: i64,
    pub bottles: HashMap<String, Bottle>,

    pub deps: Vec<String>,
    pub opt_deps: Vec<String>,
    pub rec_deps: Vec<String>,
}

impl FormulaStable {
    pub fn version_fmt(&self) -> String {
        format!("{}_{}", self.version, self.revision)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Bottle {
    pub cellar: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Repo {
    pub formulae: HashMap<String, FormulaStable>,
}

async fn read_index_cached(path: &Utf8Path) -> Result<Repo> {
    if tokio::fs::try_exists(path).await? {
        // TODO maybe do it with a reader
        let data = tokio::fs::read(path).await?;

        bincode::deserialize(&data).map_err(|e| anyhow!("deserialization error: {}", e))
    } else {
        bail!("index path does not exist")
    }
}

async fn fetch_repo() -> Result<Repo> {
    let progress = ProgressBar::new(0)
        .with_style(fetch_bar_style())
        .with_prefix(" -> fetching repo index --")
        .with_message("opening connection...");

    let mut resp = reqwest::get(FORMULAE_URL).await?;

    progress.set_length(
        resp.content_length()
            .context("no content length received")?,
    );

    let mut data: Vec<u8> = vec![];

    while let Some(chunk) = resp.chunk().await? {
        progress.inc(chunk.len() as _);
        data.extend(chunk);
    }

    let val = serde_json::from_slice(&data)?;

    parse_json_repo(&val)
}

fn parse_json_repo(val: &Value) -> Result<Repo> {
    // TODO Fix this abomination
    Ok(Repo {
        formulae: val
            .as_array()
            .context("Did not find top-level array in API answer")?
            .iter()
            .map(|form| {
                let name = form["name"].as_str().context("Package name not found")?;

                let formula = FormulaStable {
                    name: name.to_string(),
                    description: form["desc"]
                        .as_str()
                        .context("Package description not found")?
                        .to_string(),
                    version: form["versions"]["stable"]
                        .as_str()
                        .context("Stable package version not found")?
                        .to_string(),
                    revision: form["revision"].as_i64().unwrap_or(0),
                    deps: form["dependencies"]
                        .as_array()
                        .context("Dependency array not found")?
                        .iter()
                        .map(|v| {
                            Ok(v.as_str()
                                .context("Dependency is not a string")?
                                .to_string())
                        })
                        .collect::<Result<Vec<_>>>()?,
                    opt_deps: form["optional_dependencies"]
                        .as_array()
                        .context("Optional dependency array not found")?
                        .iter()
                        .map(|v| {
                            Ok(v.as_str()
                                .context("Dependency is not a string")?
                                .to_string())
                        })
                        .collect::<Result<Vec<_>>>()?,
                    rec_deps: form["recommended_dependencies"]
                        .as_array()
                        .context("Recommended dependency array not found")?
                        .iter()
                        .map(|v| {
                            Ok(v.as_str()
                                .context("Dependency is not a string")?
                                .to_string())
                        })
                        .collect::<Result<Vec<_>>>()?,
                    bottles: form["bottle"]["stable"]["files"]
                        .as_object()
                        .context("Bottle files not found")?
                        .iter()
                        .map(|(n, v)| {
                            Ok((
                                n.clone(),
                                serde_json::from_value::<Bottle>(v.clone())
                                    .context("Failed to parse bottle file value")?,
                            ))
                        })
                        .collect::<Result<HashMap<_, _>>>()
                        .context("Failed to parse bottle list")?,
                };

                Ok((name.to_string(), formula))
            })
            .collect::<Result<HashMap<_, _>>>()
            .context("Failed to parse formulae")?,
    })
}

pub async fn get_repo() -> Result<Repo> {
    create_dir_all(&*SAMOGON_DATA_DIR).await?;

    let index_path = SAMOGON_DATA_DIR.join("index.bin");

    if let Ok(cached_index) = read_index_cached(&index_path).await {
        Ok(cached_index)
    } else {
        let repo = fetch_repo().await?;

        if let Ok(bin) = bincode::serialize(&repo) {
            let _ = write(index_path, &bin).await;
            // else warn
        }
        // else warn

        Ok(repo)
    }
}
