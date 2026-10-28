use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Cli {
    _tmp: tempfile::TempDir,
    data: PathBuf,
    keys: PathBuf,
}
impl Cli {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let keys = tmp.path().join("keys");
        Self {
            _tmp: tmp,
            data,
            keys,
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self._tmp.path().join(name)
    }
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", &self.keys)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        drop(child.stdin.take());
        child.wait_with_output().unwrap()
    }
    fn ok(&self, args: &[&str], stdin: Option<&str>) -> String {
        let out = self.run(args, stdin);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}
fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
fn variants(secret: &str) -> Vec<String> {
    let percent: String = secret
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let b64 = base64(secret.as_bytes());
    vec![secret.to_owned(), percent, b64]
}
fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
    }
    out
}

#[test]
fn init_add_list_rm_flow() {
    let c = Cli::new();
    assert!(c.ok(&["vault", "init"], None).starts_with("vault\t"));
    let added = c.ok(
        &[
            "vault",
            "add",
            "github",
            "--kind",
            "login",
            "--origin",
            "https://github.com",
            "--set",
            "username=me",
            "--stdin",
        ],
        Some(r#"{"password":"KH-SYNTH-flow"}"#),
    );
    assert_eq!(added, "added\tgithub\n");
    let list = c.ok(&["vault", "list"], None);
    assert!(
        list.starts_with(
            "github\tlogin\tcredentials\tactive\thttps://github.com\tpassword,username"
        ),
        "{list}"
    );
    let json = c.ok(&["vault", "list", "--json"], None);
    assert!(json.contains("\"name\":\"github\""), "{json}");
    assert_eq!(c.ok(&["vault", "rm", "github"], None), "removed\tgithub\n");
    assert_eq!(c.ok(&["vault", "list"], None), "");
}

#[test]
fn add_rejects_secret_flags() {
    let c = Cli::new();
    c.ok(&["vault", "init"], None);
    let secret = "KH-SYNTH-flag";
    let flag = c.run(
        &[
            "vault",
            "add",
            "github",
            "--kind",
            "login",
            "--password",
            secret,
        ],
        None,
    );
    assert!(!flag.status.success());
    assert!(!text(&flag).contains(secret), "{}", text(&flag));
    let set = c.run(
        &[
            "vault",
            "add",
            "github",
            "--kind",
            "login",
            "--set",
            &format!("password={secret}"),
        ],
        None,
    );
    assert!(!set.status.success());
    assert!(
        text(&set).contains("field password is secret"),
        "{}",
        text(&set)
    );
    assert!(!text(&set).contains(secret));
    assert_eq!(c.ok(&["vault", "list"], None), "");
}

#[test]
fn secret_input_requires_terminal_or_stdin() {
    let c = Cli::new();
    c.ok(&["vault", "init"], None);
    let out = c.run(
        &[
            "vault",
            "add",
            "github",
            "--kind",
            "login",
            "--set",
            "username=me",
        ],
        None,
    );
    assert!(!out.status.success());
    assert!(
        text(&out).contains("secret input requires a terminal or --stdin"),
        "{}",
        text(&out)
    );
}

#[test]
fn no_command_prints_plaintext() {
    let c = Cli::new();
    let s = "KH-SYNTH-7f3a91c2";
    let mut transcript = String::new();
    let mut run =
        |args: &[&str], stdin: Option<&str>| transcript.push_str(&text(&c.run(args, stdin)));
    run(&["vault", "init"], None);
    run(
        &[
            "vault",
            "add",
            "github",
            "--kind",
            "login",
            "--origin",
            "https://github.com",
            "--stdin",
        ],
        Some(&format!(r#"{{"username":"{s}-user","password":"{s}-pw"}}"#)),
    );
    run(
        &["vault", "add", "openai", "--kind", "secret", "--stdin"],
        Some(&format!(r#"{{"value":"{s}-key"}}"#)),
    );
    run(
        &["vault", "add", "visa", "--kind", "card", "--stdin"],
        Some(&format!(r#"{{"number":"{s}-card","cvc":"9876"}}"#)),
    );
    run(
        &["vault", "add", "me", "--kind", "identity", "--stdin"],
        Some(&format!(r#"{{"full_name":"{s}-name"}}"#)),
    );
    run(
        &["vault", "add", "home", "--kind", "address", "--stdin"],
        Some(&format!(r#"{{"line1":"{s}-addr"}}"#)),
    );
    let env_file = c.path("app.env");
    std::fs::write(&env_file, format!("API_KEY={s}-env\n")).unwrap();
    run(
        &["vault", "import-env", "app", env_file.to_str().unwrap()],
        None,
    );
    run(
        &["vault", "edit", "github", "--stdin"],
        Some(&format!(r#"{{"password":"{s}-pw2"}}"#)),
    );
    run(&["vault", "list"], None);
    run(&["vault", "list", "--json"], None);
    let export = c.path("out.age");
    run(
        &[
            "vault",
            "export",
            export.to_str().unwrap(),
            "--passphrase-stdin",
        ],
        Some("pass phrase\n"),
    );
    run(&["vault", "rotate-root"], None);
    run(&["vault", "rotate-domain", "payment"], None);
    run(
        &["vault", "add", "github", "--kind", "login", "--stdin"],
        Some(&format!(r#"{{"password":"{s}-dup"}}"#)),
    );
    run(
        &["vault", "add", "x", "--kind", "nonsense", "--stdin"],
        Some(&format!(r#"{{"value":"{s}-bad"}}"#)),
    );
    run(
        &["vault", "add", "y", "--kind", "secret", "--stdin"],
        Some(&format!(r#"{{"value":"{s}-oops""#)),
    );
    run(&["vault", "rm", "nope"], None);
    run(&["vault", "rm", "github"], None);
    let other = Cli::new();
    transcript.push_str(&text(&other.run(&["vault", "init"], None)));
    transcript.push_str(&text(&other.run(
        &[
            "vault",
            "import",
            export.to_str().unwrap(),
            "--passphrase-stdin",
        ],
        Some("pass phrase\n"),
    )));
    transcript.push_str(&text(&other.run(&["vault", "list"], None)));
    assert!(transcript.contains("imported\t6"), "{transcript}");
    for v in variants(s) {
        assert!(!transcript.contains(&v), "leaked {v:?} in:\n{transcript}");
    }
    assert!(!transcript.contains("9876"));
}

#[test]
fn there_is_no_show_command() {
    let c = Cli::new();
    c.ok(&["vault", "init"], None);
    for cmd in ["show", "reveal", "get"] {
        assert!(
            !c.run(&["vault", cmd, "github"], None).status.success(),
            "{cmd}"
        );
    }
}

#[test]
fn missing_key_is_reported_not_recreated() {
    let c = Cli::new();
    c.ok(&["vault", "init"], None);
    std::fs::remove_dir_all(&c.keys).unwrap();
    std::fs::create_dir_all(&c.keys).unwrap();
    let out = c.run(&["vault", "list"], None);
    assert!(!out.status.success());
    assert!(text(&out).contains("vault key not found"), "{}", text(&out));
    assert_eq!(std::fs::read_dir(&c.keys).unwrap().count(), 0);
}
