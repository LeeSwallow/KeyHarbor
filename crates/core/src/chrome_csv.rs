//! Chrome's password export (`name,url,username,password,note`) for `vault import-chrome`.
//! Columns are found by the header, so their order does not matter; `url` and `password` must
//! be there. Fields follow RFC 4180 quoting (a quoted field may hold commas, newlines and `""`).
//! Error messages name lines and columns, never values.

use anyhow::{Result, anyhow, bail};
use zeroize::Zeroizing;

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

pub fn parse_chrome_csv(text: &str) -> Result<Vec<CsvLogin>> {
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
}
