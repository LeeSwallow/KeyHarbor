//! The supervisor process's command line, written and read here only:
//! `exe --data-dir DATA __supervise ID GENERATION BROWSER [--url URL] [--cdp]`.

use anyhow::{Context, Result, bail};
use keyharbor_domain::{Generation, SessionId};
use std::{ffi::OsString, path::PathBuf};

/// The hidden subcommand the binary runs a supervisor under.
pub const SUPERVISE_COMMAND: &str = "__supervise";

/// What one supervisor run is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuperviseArgs {
    pub id: SessionId,
    pub generation: Generation,
    pub browser: PathBuf,
    /// Already validated by `validate_start_url`.
    pub url: Option<String>,
    pub cdp: bool,
}

impl SuperviseArgs {
    /// The full argument list after the executable, data directory included.
    pub fn to_argv(&self, data: &std::path::Path) -> Vec<OsString> {
        let mut argv: Vec<OsString> = vec![
            "--data-dir".into(),
            data.into(),
            SUPERVISE_COMMAND.into(),
            self.id.to_string().into(),
            self.generation.to_string().into(),
            self.browser.clone().into(),
        ];
        if let Some(url) = &self.url {
            argv.extend(["--url".into(), url.into()]);
        }
        if self.cdp {
            argv.push("--cdp".into());
        }
        argv
    }

    /// Reads the arguments after [`SUPERVISE_COMMAND`].
    pub fn parse(args: &[OsString]) -> Result<Self> {
        let mut positional = Vec::new();
        let (mut url, mut cdp) = (None, false);
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--url") => {
                    let value = args.next().context("--url needs a value")?;
                    url = Some(value.to_str().context("--url is not UTF-8")?.to_owned());
                }
                Some("--cdp") => cdp = true,
                _ => positional.push(arg),
            }
        }
        let [id, generation, browser] = positional[..] else {
            bail!("expected ID GENERATION BROWSER");
        };
        Ok(Self {
            id: SessionId::parse(id.to_str().context("invalid session id")?)
                .context("invalid session id")?,
            generation: Generation::parse(generation.to_str().context("invalid generation")?)
                .context("invalid generation")?,
            browser: PathBuf::from(browser),
            url,
            cdp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_argv_reads_back_as_written() {
        let args = SuperviseArgs {
            id: keyharbor_crypto::random::id::<SessionId>(),
            generation: keyharbor_crypto::random::id::<Generation>(),
            browser: PathBuf::from("/bin/chrome"),
            url: Some("https://example.com/".into()),
            cdp: true,
        };
        let argv = args.to_argv(std::path::Path::new("/data"));
        assert_eq!(
            argv[..3],
            [
                OsString::from("--data-dir"),
                "/data".into(),
                "__supervise".into()
            ]
        );
        assert_eq!(SuperviseArgs::parse(&argv[3..]).unwrap(), args);
        let plain = SuperviseArgs {
            url: None,
            cdp: false,
            ..args
        };
        assert_eq!(
            SuperviseArgs::parse(&plain.to_argv(std::path::Path::new("/d"))[3..]).unwrap(),
            plain
        );
        assert!(SuperviseArgs::parse(&["x".into()]).is_err());
    }
}
