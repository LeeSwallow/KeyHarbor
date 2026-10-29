//! `keyharbor run`: hand vault values to one child process and mask its output (ADR-0010,
//! developer secrets spec §3). Values go as variables (`--env`, `--env-file`), as files
//! (`--file`, `--template`, [`files`]) or into the command's arguments (`{{secret:…}}`).
//!
//! Every referenced item is checked first (exists, not discarded, allowed for the agent, not
//! origin-bound, `run_allowed`); a refusal is recorded and the command is not started. The
//! child's stdout and stderr pass through a masker built from the whole vault as they stream
//! ([`output`]); stop signals are passed on to it ([`signals`]).

mod child;
mod files;
mod output;
mod signals;

use crate::compose::App;
use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use files::SecretFiles;
use keyharbor_domain::{
    policy::AgentName,
    vault::{FieldName, PlaceholderRef, has_placeholder, parse_placeholders, placeholder},
};
use keyharbor_guard::run as guard_run;
use std::{ffi::OsString, path::PathBuf};
use zeroize::Zeroizing;

#[derive(Args)]
pub struct RunArgs {
    /// Agent name for policies and the usage record
    #[arg(long, default_value = keyharbor_core::LOCAL_AGENT)]
    agent: String,
    /// Inject every variable of this env item
    #[arg(long = "env", value_name = "ITEM")]
    items: Vec<String>,
    /// Inject KEY=VALUE lines whose values may hold {{secret:item.field}} placeholders
    #[arg(long = "env-file", value_name = "PATH")]
    files: Vec<PathBuf>,
    /// Write one field to a private file and put its path in VAR, e.g.
    /// --file GOOGLE_APPLICATION_CREDENTIALS=gcp.key
    #[arg(long = "file", value_name = "VAR=ITEM.FIELD")]
    secret_files: Vec<String>,
    /// Fill the placeholders in a file into a private copy and put its path in VAR
    #[arg(long = "template", value_name = "VAR=PATH")]
    templates: Vec<String>,
    /// The command and its arguments, after `--`. Arguments may hold placeholders (other
    /// processes of yours can read arguments, so prefer --env or --file)
    #[arg(last = true, required = true, value_name = "COMMAND")]
    command: Vec<OsString>,
}

/// `VAR=rest`, with VAR a valid environment variable name.
fn assignment(text: &str) -> Result<(String, &str)> {
    let (var, rest) = text.split_once('=').context("expected VAR=…")?;
    let var = FieldName::parse_env(var).map_err(|e| anyhow!("{e}"))?;
    Ok((var.as_str().to_owned(), rest))
}

/// `--file VAR=item.field`.
fn secret_file(text: &str) -> Result<(String, PlaceholderRef)> {
    let (var, reference) = assignment(text)?;
    let refs = parse_placeholders(&placeholder_text(reference)).map_err(|e| anyhow!("{e}"))?;
    let [reference] = <[PlaceholderRef; 1]>::try_from(refs)
        .map_err(|_| anyhow!("--file expects VAR=ITEM.FIELD"))?;
    Ok((var, reference))
}

/// `item.field` as the placeholder the parser reads.
fn placeholder_text(reference: &str) -> String {
    match reference.split_once('.') {
        Some((item, field)) => placeholder(item, field),
        None => reference.to_owned(),
    }
}

/// `--template VAR=PATH`: the variable and the file's contents (which may hold values once
/// filled, so they are zeroized on drop).
fn template(text: &str) -> Result<(String, Zeroizing<String>)> {
    let (var, path) = assignment(text)?;
    let contents = std::fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
    Ok((var, Zeroizing::new(contents)))
}

/// The `--env-file` templates; they may hold values, so they are zeroized on drop.
fn read_env_files(paths: &[PathBuf]) -> Result<Vec<Zeroizing<String>>> {
    paths
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .map(Zeroizing::new)
                .with_context(|| format!("cannot read {}", path.display()))
        })
        .collect()
}

/// The command's arguments after the program, as text (placeholders are text).
fn arguments(command: &[OsString]) -> Result<Vec<String>> {
    command[1..]
        .iter()
        .map(|arg| {
            arg.to_str()
                .map(str::to_owned)
                .context("arguments must be UTF-8")
        })
        .collect()
}

/// Runs the command and returns the exit code to pass on (128 + signal if it was killed).
pub fn run(app: &App, args: RunArgs) -> Result<i32> {
    let agent = AgentName::parse(&args.agent).map_err(|e| anyhow!("{e}"))?;
    let command_args = arguments(&args.command)?;
    let nothing_given = args.items.is_empty()
        && args.files.is_empty()
        && args.secret_files.is_empty()
        && args.templates.is_empty()
        && !command_args.iter().any(|a| has_placeholder(a));
    if nothing_given {
        bail!("give --env, --env-file, --file, --template or a placeholder in an argument");
    }
    let files = read_env_files(&args.files)?;
    let secret_files = args
        .secret_files
        .iter()
        .map(|f| secret_file(f))
        .collect::<Result<Vec<_>>>()?;
    let templates = args
        .templates
        .iter()
        .map(|t| template(t))
        .collect::<Result<Vec<_>>>()?;
    let (store, vault) = (app.store()?, app.vault()?);
    // The guard checks every item before resolving any, and builds the output masker.
    let mut injection = guard_run::prepare(
        &*store,
        &vault,
        &guard_run::Request {
            agent: agent.as_str(),
            items: &args.items,
            files: &files,
            secret_files: &secret_files,
            templates: &templates,
            args: &command_args,
        },
    )?;
    if injection.args_hold_values {
        eprintln!(
            "warning: values in arguments can be read by your other processes (ps); prefer --env or --file"
        );
    }
    let (held_files, paths) = SecretFiles::write(&std::mem::take(&mut injection.files))?;
    let mut vars = std::mem::take(&mut injection.vars);
    vars.extend(paths);
    let child_args = std::mem::take(&mut injection.args);
    signals::block_forwarded()?;
    let mut child = child::spawn(&args.command[0], &child_args, &vars)?;
    signals::forward_signals(child.id());
    drop((vars, child_args));
    injection.record_injected(&*store);
    let (out, err) = (
        child.stdout.take().context("child stdout")?,
        child.stderr.take().context("child stderr")?,
    );
    let status = std::thread::scope(|scope| {
        scope.spawn(|| output::pump(out, std::io::stdout().lock(), &injection.masker));
        scope.spawn(|| output::pump(err, std::io::stderr().lock(), &injection.masker));
        child.wait()
    })?;
    drop(held_files);
    use std::os::unix::process::ExitStatusExt;
    Ok(status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_and_template_arguments_name_a_variable() {
        let (var, reference) = secret_file("GOOGLE_APPLICATION_CREDENTIALS=gcp.key").unwrap();
        assert_eq!(var, "GOOGLE_APPLICATION_CREDENTIALS");
        assert_eq!(reference.to_string(), "{{secret:gcp.key}}");
        for bad in [
            "gcp.key",
            "lower=gcp.key",
            "X=gcp",
            "X=gcp.key.more",
            "X=a b.key",
        ] {
            assert!(secret_file(bad).is_err(), "{bad}");
        }
        assert!(template("X=/no/such/file").is_err());
    }
}
