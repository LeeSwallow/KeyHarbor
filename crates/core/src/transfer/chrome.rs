//! Chrome's password export (`name,url,username,password,note`) for `vault import-chrome`.
//! Columns are found by the header, so their order does not matter; `url` and `password` must
//! be there. Fields follow RFC 4180 quoting (a quoted field may hold commas, newlines and `""`).
//! Error messages name lines and columns, never values.

use crate::{ItemSource, ItemStatus, NewItem, Vault};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::vault::{FieldName, ItemKind, ItemName, Origin};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// What `import_chrome_csv` did. Counts only: names and values stay out of reports.
#[derive(Debug, PartialEq, Eq)]
pub struct ChromeImportReport {
    pub added: usize,
    /// Rows whose name an item already had; that item is left as it was.
    pub skipped_existing: usize,
    /// Rows for no web address (`android://…`, plain HTTP): an origin cannot hold them.
    pub skipped_not_web: usize,
}

/// Items from a Chrome export are named `chrome/<host>`, dots as `-`: `chrome/github-com`.
const CHROME_PREFIX: &str = "chrome/";
/// Room left for a `-<n>` suffix within the 64-character name limit.
const CHROME_HOST_CHARS: usize = 52;

/// `chrome/<host>`, or `chrome/<host>-<n>` for the n-th account on that site in this file.
fn chrome_item_name(origin: &Origin, seen: &mut BTreeMap<String, usize>) -> Result<ItemName> {
    let host = origin.as_str().split_once("://").map_or("", |(_, h)| h);
    let host: String = host
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(CHROME_HOST_CHARS)
        .collect();
    let count = seen.entry(host.clone()).or_default();
    *count += 1;
    let name = match *count {
        1 => format!("{CHROME_PREFIX}{host}"),
        n => format!("{CHROME_PREFIX}{host}-{n}"),
    };
    Ok(ItemName::parse(&name)?)
}

impl Vault {
    /// Adds a login for each row of a Chrome password export, bound to the row's site, with
    /// the note in the login's secret `note` field. Existing names are not overwritten.
    pub fn import_chrome_csv(&self, text: &str) -> Result<ChromeImportReport> {
        let rows = parse_chrome_csv(text)?;
        let existing: Vec<String> = self.list()?.into_iter().map(|i| i.name).collect();
        let mut report = ChromeImportReport {
            added: 0,
            skipped_existing: 0,
            skipped_not_web: 0,
        };
        let mut seen = BTreeMap::new();
        for row in rows {
            let Ok(origin) = Origin::of_page(&row.url) else {
                report.skipped_not_web += 1;
                continue;
            };
            let name = chrome_item_name(&origin, &mut seen)?;
            if existing.iter().any(|e| e == name.as_str()) {
                report.skipped_existing += 1;
                continue;
            }
            let mut fields = Vec::new();
            for (field, value) in [
                ("username", row.username),
                ("password", row.password),
                ("note", row.note),
            ] {
                if !value.is_empty() {
                    fields.push((FieldName::parse(field)?, value));
                }
            }
            self.add(NewItem {
                name,
                kind: ItemKind::Login,
                fields,
                origins: vec![origin],
                origin_binding: None,
                status: ItemStatus::Active,
                source: ItemSource::Import,
                details: Default::default(),
            })
            .with_context(|| format!("line {}", row.line))?;
            report.added += 1;
        }
        Ok(report)
    }
}

/// One saved sign-in from the export. `line` is where its row starts, for messages.
pub struct CsvLogin {
    pub line: usize,
    pub url: String,
    pub username: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub note: Zeroizing<String>,
}

/// The rows of `text` as fields, each with the line it starts on. Values are zeroized on drop.
fn records(text: &str) -> Result<Vec<(usize, Vec<Zeroizing<String>>)>> {
    let mut out = Vec::new();
    let (mut fields, mut field) = (Vec::new(), Zeroizing::new(String::new()));
    let (mut line, mut start) = (1, 1);
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if c == '\n' {
            line += 1;
        }
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => fields.push(std::mem::take(&mut field)),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n') => {
                fields.push(std::mem::take(&mut field));
                out.push((start, std::mem::take(&mut fields)));
                start = line;
            }
            (false, c) => field.push(c),
        }
    }
    if quoted {
        bail!("line {start}: a quoted field is not closed");
    }
    if !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        out.push((start, fields));
    }
    Ok(out)
}

fn parse_chrome_csv(text: &str) -> Result<Vec<CsvLogin>> {
    let mut rows = records(text)?.into_iter();
    let (_, header) = rows.next().ok_or_else(|| anyhow!("the file is empty"))?;
    let column = |name: &str| header.iter().position(|h| h.trim() == name);
    let required = |name: &str| {
        column(name).ok_or_else(|| {
            anyhow!("the header has no {name} column (expected name,url,username,password,note)")
        })
    };
    let (url, password) = (required("url")?, required("password")?);
    let (username, note) = (column("username"), column("note"));
    let mut out = Vec::new();
    for (line, mut fields) in rows {
        if fields.len() != header.len() {
            bail!(
                "line {line}: expected {} fields, found {}",
                header.len(),
                fields.len()
            );
        }
        let mut take = |at: Option<usize>| {
            at.map(|i| std::mem::take(&mut fields[i]))
                .unwrap_or_default()
        };
        out.push(CsvLogin {
            line,
            url: take(Some(url)).to_string(),
            password: take(Some(password)),
            username: take(username),
            note: take(note),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, init, login, pref};

    const HEADER: &str = "name,url,username,password,note\n";

    #[test]
    fn rows_are_read_by_their_header_names() {
        let text = "url,password,username\nhttps://a.example/login,pw1,amy\n";
        let rows = parse_chrome_csv(text).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (
                rows[0].url.as_str(),
                rows[0].username.as_str(),
                rows[0].password.as_str()
            ),
            ("https://a.example/login", "amy", "pw1")
        );
        assert_eq!((rows[0].line, rows[0].note.as_str()), (2, ""));
    }

    #[test]
    fn quoted_fields_keep_commas_quotes_and_newlines() {
        let text = format!(
            "{HEADER}github.com,https://github.com/,\"a,b\",\"p\"\"w\",\"two\nlines\"\nbank,https://bank.example/,c,d,\n"
        );
        let rows = parse_chrome_csv(&text).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].username.as_str(), "a,b");
        assert_eq!(rows[0].password.as_str(), "p\"w");
        assert_eq!(rows[0].note.as_str(), "two\nlines");
        assert_eq!(
            rows[1].line, 4,
            "the second row starts after the two-line note"
        );
    }

    #[test]
    fn windows_line_ends_and_a_missing_last_newline_are_fine() {
        let rows = parse_chrome_csv("url,password\r\nhttps://a.example/,x\r\nhttps://b.example/,y")
            .unwrap();
        assert_eq!(
            rows.iter().map(|r| r.password.as_str()).collect::<Vec<_>>(),
            ["x", "y"]
        );
    }

    #[test]
    fn errors_name_the_line_and_never_a_value() {
        let missing = parse_chrome_csv("name,username\nx,y\n")
            .err()
            .unwrap()
            .to_string();
        assert_eq!(
            missing,
            "the header has no url column (expected name,url,username,password,note)"
        );
        let short = parse_chrome_csv(&format!("{HEADER}n,https://a.example/,u\n"))
            .err()
            .unwrap()
            .to_string();
        assert_eq!(short, "line 2: expected 5 fields, found 3");
        let open = parse_chrome_csv(&format!("{HEADER}n,https://a.example/,u,\"secret-pw,\n"))
            .err()
            .unwrap()
            .to_string();
        assert_eq!(open, "line 2: a quoted field is not closed");
        for message in [missing, short, open] {
            assert!(!message.contains("secret-pw"));
        }
    }

    #[test]
    fn a_chrome_export_becomes_one_login_per_row_bound_to_its_site() {
        let e = env();
        let v = init(&e);
        v.add(login("chrome/bank-example", "kept")).unwrap();
        let csv = "name,url,username,password,note\n\
            github.com,https://github.com/login,amy,pw-1,\"recovery\ncodes\"\n\
            github.com,https://github.com/,bob,pw-2,\n\
            bank,https://bank.example/,c,pw-3,\n\
            app,android://x@com.example/,d,pw-4,\n";
        let report = v.import_chrome_csv(csv).unwrap();
        assert_eq!(
            report,
            super::ChromeImportReport {
                added: 2,
                skipped_existing: 1,
                skipped_not_web: 1
            }
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/github-com", "password")).unwrap(),
            "pw-1"
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/github-com", "note")).unwrap(),
            "recovery\ncodes"
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/github-com-2", "username")).unwrap(),
            "bob"
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/bank-example", "password")).unwrap(),
            "kept",
            "not overwritten"
        );
        let github = v
            .list()
            .unwrap()
            .into_iter()
            .find(|i| i.name == "chrome/github-com")
            .unwrap();
        assert_eq!(
            (github.kind, github.source),
            (ItemKind::Login, ItemSource::Import)
        );
        assert_eq!(github.origins, ["https://github.com"]);
        assert!(
            github.description.is_none(),
            "the note is a secret field, not a plaintext description"
        );
    }
}
