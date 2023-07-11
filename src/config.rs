use camino::{Utf8Path, Utf8PathBuf};
use lazy_static::lazy_static;

pub const FETCH_RETRIES: u64 = 3;
pub const MAX_CONCURRENT_FETCHES: usize = 16;

lazy_static! {
    pub static ref HOMEBREW_CACHE: Utf8PathBuf = {
        std::env::var("HOMEBREW_CACHE")
            .map(|x| x.into())
            .unwrap_or_else(|_| {
                Utf8PathBuf::from(std::env::var("HOME").expect("HOME is undefined"))
                    .join(Utf8Path::new("Library/Caches/Homebrew"))
            })
    };
    pub static ref HOMEBREW_PREFIX: Utf8PathBuf = {
        std::env::var("HOMEBREW_PREFIX")
            .unwrap_or_else(|_| "/opt/homebrew".to_owned())
            .into()
    };
    pub static ref SAMOGON_DATA_DIR: Utf8PathBuf = { HOMEBREW_PREFIX.join(".samogon") };
}
