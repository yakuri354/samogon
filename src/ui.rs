use std::cmp::min;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use dialoguer::Confirm;
use indicatif::ProgressStyle;
use itertools::Itertools;
use textwrap::{core::Word, WrapAlgorithm};
use tokio::task::spawn_blocking;

use crate::repo::FormulaStable;

pub const PROGRESS_CHARS: &'static str = "━╾╴─";

pub async fn confirm_install(pkgs: &[FormulaStable]) -> Result<bool> {
    if pkgs.len() == 0 {
        bail!("nothing to confirm");
    }

    print!("\n\n");

    let (w, _) = term_size::dimensions().context("could not get terminal size")?;
    let prefix = format!(" -> will install {} pkgs: ", pkgs.len());
    print!("{}", prefix);
    let mut tabs = prefix.len() as i64;
    let mut wrap_w = w as i64 - tabs - 2;

    if wrap_w < 20 {
        print!("\n\n");
    }

    if 0 < wrap_w && wrap_w < 20 {
        tabs = 4;
        wrap_w = w as i64 - 6;
        print!("{}", " ".repeat(4));
    } else if wrap_w <= 0 {
        tabs = 0;
        wrap_w = w as _;
    }

    // wrap_w = min(wrap_w, 100i64);

    let pieces = pkgs
        .iter()
        .map(|p| format!("{} of {}, ", p.name.bold(), p.version_fmt().green()))
        .collect::<Vec<_>>();
    let words = pieces.iter().map(|s| Word::from(s)).collect::<Vec<_>>();

    let algo = WrapAlgorithm::new_optimal_fit();

    let line_lengths = vec![wrap_w as usize];

    let lines = algo.wrap(&words, &line_lengths);

    let mut text_it = lines.iter().map(|l| l.iter().map(|w| w.word).join(" "));

    println!("{}", text_it.next().unwrap());

    let text = text_it
        .map(|l| format!("{}{}", " ".repeat(tabs as _), l))
        .join("\n");

    println!("{}\n\n", text);

    // TODO maybe estimate total size

    let result = spawn_blocking(move || {
        Confirm::new()
            .with_prompt(" proceed?")
            .default(true)
            .interact()
    })
    .await??;

    Ok(result)
}

pub fn total_bar_style() -> ProgressStyle {
    ProgressStyle::with_template(
        // "  total {wide_msg:<} after {elapsed} got {bytes:<7.green} eta {eta:.blue}    {bar:60.green/black}"
        "\n\n  {elapsed} elapsed {wide_msg:<} {pos} out of {len}    {bar:50.green/black}",
    )
    .unwrap()
    .progress_chars(PROGRESS_CHARS)
}

pub fn fetch_bar_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "{prefix:<20.bold} {wide_msg:<} got {decimal_bytes:<15.green} at {bytes_per_sec:<15.bold} eta {eta:<4.blue}    {bar:50.green/black}",
    )
    .unwrap()
    .progress_chars(PROGRESS_CHARS)
}

pub fn common_bar_prefix() -> String {
    " ".repeat(2)
}
