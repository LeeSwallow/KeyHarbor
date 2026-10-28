//! `screenshot`: a PNG of the active tab, saved privately under `run/shots/`.

use super::CdpBrowser;
use crate::paths;
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{Value, json};

impl CdpBrowser {
    pub(super) async fn save_screenshot(&mut self) -> Result<Value> {
        let session = self.active_page().await?;
        let shot = self
            .call("Page.captureScreenshot", json!({"format": "png"}), &session)
            .await?;
        let png = base64::engine::general_purpose::STANDARD
            .decode(
                shot["data"]
                    .as_str()
                    .context("screenshot returned no data")?,
            )
            .context("screenshot data is not base64")?;
        let dir = paths::shots_dir(&self.data);
        paths::private_dir(dir.parent().context("invalid shots path")?)?;
        paths::private_dir(&dir)?;
        for n in 1.. {
            let path = dir.join(format!("{}-{n}.png", self.session));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    std::io::Write::write_all(&mut file, &png)?;
                    return Ok(json!({"path": path}));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!()
    }
}
