//! The supervisor process's command line, written and read here only:
//! `exe --data-dir DATA __supervise ID GENERATION BROWSER --call-seconds N [--url URL] [--cdp]
//! [--ego-lite] [--headless]`.

use anyhow::{Context, Result, bail};
use keyharbor_domain::{Generation, SessionId, browser_choice::BrowserKind};
use std::{ffi::OsString, path::PathBuf, time::Duration};

/// The hidden subcommand the binary runs a supervisor under.
pub const SUPERVISE_COMMAND: &str = "__supervise";
const CALL_SECONDS: &str = "--call-seconds";

/// What one supervisor run is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuperviseArgs {
    pub id: SessionId,
    pub generation: Generation,
    pub browser: PathBuf,
    /// Already validated by `validate_start_url`.
    pub url: Option<String>,
    pub cdp: bool,
    pub kind: BrowserKind,
    pub headless: bool,
    /// How long one CDP call (and ego lite's relay connection) may take: the
    /// `browser_call_seconds` setting as the session opened, in whole seconds (J-272).
    pub call_timeout: Duration,
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
            CALL_SECONDS.into(),
            self.call_timeout.as_secs().to_string().into(),
        ];
        if let Some(url) = &self.url {
            argv.extend(["--url".into(), url.into()]);
        }
        if self.cdp {
            argv.push("--cdp".into());
        }
        if self.kind == BrowserKind::EgoLite {
            argv.push("--ego-lite".into());
        }
        if self.headless {
            argv.push("--headless".into());
        }
        argv
    }

    /// Reads the arguments after [`SUPERVISE_COMMAND`].
    pub fn parse(args: &[OsString]) -> Result<Self> {
        let mut positional = Vec::new();
        let (mut url, mut cdp, mut kind, mut headless) = (None, false, BrowserKind::Chrome, false);
        let mut call_seconds = None;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--url") => {
                    let value = args.next().context("--url needs a value")?;
                    url = Some(value.to_str().context("--url is not UTF-8")?.to_owned());
                }
                Some(CALL_SECONDS) => {
                    let value = args.next().and_then(|v| v.to_str()?.parse::<u64>().ok());
                    call_seconds = Some(value.context("--call-seconds needs whole seconds")?);
                }
                Some("--cdp") => cdp = true,
                Some("--ego-lite") => kind = BrowserKind::EgoLite,
                Some("--headless") => headless = true,
                _ => positional.push(arg),
            }
        }
        let [id, generation, browser] = positional[..] else {
            bail!("expected ID GENERATION BROWSER");
        };
        let call_seconds = call_seconds.context("expected --call-seconds")?;
        Ok(Self {
            id: SessionId::parse(id.to_str().context("invalid session id")?)
                .context("invalid session id")?,
            generation: Generation::parse(generation.to_str().context("invalid generation")?)
                .context("invalid generation")?,
            browser: PathBuf::from(browser),
            url,
            cdp,
            kind,
            headless,
            call_timeout: Duration::from_secs(call_seconds),
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
            kind: BrowserKind::EgoLite,
            headless: true,
            call_timeout: Duration::from_secs(45),
        };
        let argv = args.to_argv(std::path::Path::new("/data"));
        let at = argv.iter().position(|a| a == CALL_SECONDS).unwrap();
        assert_eq!(
            argv[at + 1],
            "45",
            "the CDP call limit goes to the supervisor"
        );
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
            kind: BrowserKind::Chrome,
            headless: false,
            ..args
        };
        assert_eq!(
            SuperviseArgs::parse(&plain.to_argv(std::path::Path::new("/d"))[3..]).unwrap(),
            plain
        );
        assert!(SuperviseArgs::parse(&["x".into()]).is_err());
        let mut without_limit = plain.to_argv(std::path::Path::new("/d"));
        without_limit.drain(6..8);
        assert!(SuperviseArgs::parse(&without_limit[3..]).is_err());
    }
}
