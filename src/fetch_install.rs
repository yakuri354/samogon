use anyhow::{anyhow, bail, Context, Result};
use async_compression::tokio::bufread::GzipDecoder;
use camino::{Utf8Path, Utf8PathBuf};
use colored::Colorize;
use indicatif::{MultiProgress, MultiProgressAlignment, ProgressBar, ProgressDrawTarget};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::os::unix::prelude::MetadataExt;
use std::time::{self, Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, SeekFrom};
use tokio::task::JoinSet;
use tokio_stream::StreamExt;
use tokio_tar::Archive;

use crate::config::{FETCH_RETRIES, HOMEBREW_CACHE, MAX_CONCURRENT_FETCHES};
use crate::platform::get_current_platform;
use crate::repo::FormulaStable;
use crate::ui::{common_bar_prefix, fetch_bar_style, total_bar_style};
use crate::util::{file_digest, fmt_digest, normalize_path};

const GITHUB_AUTH_TOKEN: &str = "QQ==";

// TODO change all string paths to PathBufs

#[derive(Error, Debug)]
enum BottleFetchErr {
    #[error("bottle download is corrupted")]
    DownloadCorrupted,
}

async fn check_cached(path: &Utf8Path, checksum: &str, progress: &mut ProgressBar) -> Result<bool> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(path)
        .await?;

    let size = file.metadata().await?.size();

    progress.set_message("verifying integrity...");
    progress.set_length(size);
    progress.set_position(0);

    Ok(fmt_digest(file_digest(&mut file, progress).await?) == checksum)
}

/// Fetches the file and returns its checksum
async fn github_get(
    url: &str,
    path: &Utf8Path,
    resume: bool,
    checksum: &str,
    progress: &mut ProgressBar,
) -> Result<()> {
    let mut hash_ctx = Sha256::new();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .await?;

    if !resume {
        file.set_len(0).await?;
    }

    let offset = file.metadata().await?.size();

    file.seek(SeekFrom::Start(0)).await?;

    if offset > 0 && resume {
        progress.println(format!("resuming {}", url));
        progress.set_message("resuming download...");
    } else {
        progress.set_position(0);
        progress.set_message("opening connection...");
    }

    let mut resp = Client::new()
        .get(url)
        .bearer_auth(GITHUB_AUTH_TOKEN)
        .header("Range", format!("bytes: {}-", offset))
        .send()
        .await?;

    let size = resp
        .content_length()
        .context("No Content-Length received")?;

    progress.set_message("");
    progress.set_length(size);
    progress.set_position(offset);

    file.seek(SeekFrom::End(0)).await?;
    file.set_len(size).await?;

    while let Some(chunk) = resp.chunk().await? {
        progress.inc(chunk.len() as _);
        hash_ctx.update(&chunk);
        file.write_all(&chunk).await?;
    }

    file.seek(SeekFrom::Start(0)).await?;

    if fmt_digest(hash_ctx) == checksum {
        Ok(())
    } else {
        Err(BottleFetchErr::DownloadCorrupted.into())
    }
}

/// Fetches a bottle or gets it from cache and returns its local path
///
/// TODO multithreaded fetching of the same big bottle
async fn fetch_bottle(
    platform: &str,
    formula: &FormulaStable,
    progress: &mut ProgressBar,
) -> Result<Utf8PathBuf> {
    progress.set_message("searching cache...");

    let bottle_entry = (&formula.bottles).get(platform).context(anyhow!(
        "Package {} is unavailable for {}",
        formula.name,
        platform
    ))?;

    let url = bottle_entry.url.as_str();

    let basename = format!(
        "{}--{}.{}.bottle.tar.gz",
        &formula.name,
        formula.version_fmt(),
        platform
    );

    let url_digest = sha256::digest(url);

    // We can assume that paths are strings since only macOS is supported
    let downloads_cache = HOMEBREW_CACHE.join("downloads");

    let cache_path = downloads_cache.join(format!("{}--{}", url_digest, basename));

    let incomplete_path =
        cache_path.with_file_name(format!("{}.incomplete", cache_path.file_name().unwrap()));

    let checksum = &bottle_entry.sha256;

    match check_cached(&cache_path, checksum, progress).await {
        Ok(true) => return Ok(cache_path),
        Ok(false) => tokio::fs::remove_file(&cache_path)
            .await
            .context("while removing corrupted cache entry")?,
        _ => {} // Err(e) => progress.println(format!(" !! cache check failed due to {:?}", e)),
    }

    if let Ok(_) = github_get(&url, &incomplete_path, true, checksum, progress).await {
        tokio::fs::rename(&incomplete_path, &cache_path)
            .await
            .context("while moving incomplete -> cache")?;
        return Ok(cache_path);
    }
    // else warn(resuming failed)

    for _ in 1..FETCH_RETRIES {
        if let Ok(_) = github_get(&url, &incomplete_path, false, checksum, progress).await {
            tokio::fs::rename(&incomplete_path, &cache_path)
                .await
                .context("while moving incomplete -> cache")?;
            return Ok(cache_path);
        }
        // else warn(retry {i})
    }

    // return the error on the last try

    github_get(&url, &incomplete_path, false, checksum, progress)
        .await
        .context(anyhow!("while getting {} from github", formula.name))?;

    tokio::fs::rename(&incomplete_path, &cache_path)
        .await
        .context("while moving incomplete -> cache")?;
    Ok(cache_path)
}

async fn unpack_archive(path: &Utf8Path, progress: &mut ProgressBar) -> Result<Utf8PathBuf> {
    progress.set_position(0);
    progress.set_length(0);
    progress.set_message("unpacking bottle archive");

    let file = File::open(path).await.context("while opening archive")?;
    progress.set_length(file.metadata().await?.len() as _);
    let progress_read = progress.wrap_async_read(file);
    let reader = BufReader::with_capacity(1 << 18, progress_read);
    // TODO check if the file is really gzipped and maybe support other compression algos
    let gz_reader = GzipDecoder::new(reader);
    let mut archive = Archive::new(gz_reader);
    let mut ents = archive.entries().context("while unpacking archive")?;

    let unpack_dir: Utf8PathBuf = Utf8PathBuf::from_path_buf(std::env::temp_dir())
        .unwrap()
        .join(sha256::digest(format!(
            "{}{}",
            path.to_string(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string()
        )));

    tokio::fs::create_dir_all(&unpack_dir)
        .await
        .context("while creating cache directory")?;

    while let Some(ent) = ents.next().await {
        ent.context("while uncompressing archive item")?
            .unpack_in(&unpack_dir)
            .await?;
    }

    Ok(unpack_dir)
}

async fn pour_bottle(
    formula: &FormulaStable,
    path: &Utf8Path,
    progress: &mut ProgressBar,
) -> Result<()> {
    let tmp = unpack_archive(path, progress).await?;

    Ok(())
}

async fn stream_one(formula: FormulaStable, screen: MultiProgress) -> Result<()> {
    let platform = get_current_platform();

    let mut progress = screen.insert_from_back(
        1,
        ProgressBar::new(100)
            .with_style(fetch_bar_style())
            .with_prefix(format!(
                "{}{} {}",
                common_bar_prefix(),
                formula.name,
                formula.version_fmt()
            ))
            .with_position(0)
    );

    progress.tick();

    let path = fetch_bottle(platform, &formula, &mut progress)
        .await
        .context(anyhow!("while fetching {}", formula.name))?;

    pour_bottle(&formula, &path, &mut progress)
        .await
        .context(anyhow!("while pouring {}", formula.name))?;

    screen.remove(&progress);
    let progress = screen.insert(0, progress);

    progress.finish_with_message("done");

    Ok(())
}

pub async fn stream_all(formulae: Vec<FormulaStable>) -> Result<()> {
    let progress = MultiProgress::new();
    progress.set_alignment(MultiProgressAlignment::Top);

    let total_bar = progress.add(
        ProgressBar::new(formulae.len() as _) // TODO this is wrong.
            .with_style(total_bar_style()),
    );

    total_bar.enable_steady_tick(Duration::from_millis(500));

    let mut js = JoinSet::new();

    let mut iter = formulae.into_iter();

    for _ in 0..MAX_CONCURRENT_FETCHES {
        if let Some(i) = iter.next() {
            js.spawn(stream_one(i, progress.clone()));
        } else {
            break;
        }
    }

    while let Some(res) = js.join_next().await {
        match res? {
            Ok(_) => match iter.next() {
                Some(f) => {
                    total_bar.inc(1);
                    js.spawn(stream_one(f, progress.clone()));
                }
                None => {}
            },
            Err(e) => {
                progress.println(format!(
                    "{} {} due to the following error:\n{:?}",
                    "!".bold(),
                    "aborting".bold().red(),
                    e
                ))?;
                js.abort_all();
            }
        }
    }

    Ok(())
}
