//! Serves design-system/ (and the app icon it shows) on http://127.0.0.1:8765 so design-system/index.html can be opened in a browser
//! (ES modules and CSS masks do not load from file://). `design-serve [port]`.

use std::{env, thread};

fn main() -> std::io::Result<()> {
    let port = env::args().nth(1).unwrap_or_else(|| "8765".into());
    let base = keyharbor_design_check::serve::spawn(
        keyharbor_design_check::repo_root(),
        &format!("127.0.0.1:{port}"),
    )?;
    println!("{base}/design-system/index.html  (Ctrl-C to stop)");
    loop {
        thread::park();
    }
}
