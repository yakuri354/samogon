use anyhow::Result;
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use indicatif::ProgressBar;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub fn normalize_path(path: &Utf8Path) -> Utf8PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Utf8Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        Utf8PathBuf::from(c.as_str())
    } else {
        Utf8PathBuf::new()
    };

    for component in components {
        match component {
            Utf8Component::Prefix(..) => unreachable!(),
            Utf8Component::RootDir => {
                ret.push(component.as_str());
            }
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                ret.pop();
            }
            Utf8Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

pub fn fmt_digest(ctx: Sha256) -> String {
    hex::encode(ctx.finalize())
}

pub async fn file_digest(file: &mut File, progress: &mut ProgressBar) -> Result<Sha256> {
    let mut buf = vec![0u8; 1 << 20];
    let mut hash_ctx = Sha256::new();

    while let Ok(bytes) = file.read(&mut buf).await {
        if bytes == 0 {
            break;
        }

        hash_ctx.update(&buf[..bytes]);
        progress.inc(bytes as _);
    }

    Ok(hash_ctx)
}
