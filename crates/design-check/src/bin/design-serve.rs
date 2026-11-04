//! Serves design-system/ (and the app icon it shows) on http://127.0.0.1:8765 so design-system/index.html can be opened in a browser
//! (ES modules and CSS masks do not load from file://). `design-serve [port]`.

use std::{env, thread};

use keyharbor_design_check::DESIGN_ROOT;

/// The port when none is given; design-system/README.md links to it.
const DEFAULT_PORT: &str = "8765";

fn main() -> std::io::Result<()> {
    let port = env::args().nth(1).unwrap_or_else(|| DEFAULT_PORT.into());
    let base = keyharbor_design_check::serve::spawn(
        keyharbor_design_check::repo_root(),
        &format!("127.0.0.1:{port}"),
    )?;
    println!("{base}/{DESIGN_ROOT}/index.html  (Ctrl-C to stop)");
    loop {
        thread::park();
    }
}
