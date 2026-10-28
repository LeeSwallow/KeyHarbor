//! Origin binding (03 §3.8, J-12, J-34): where an item's values may go.

use keyharbor_domain::vault::{Domain, Origin};

/// An origin-bound item may be typed only into a frame whose origin is exactly one of the
/// item's origins (no subdomain wildcards). Unbound items go anywhere.
pub fn allows_frame(origin_binding: bool, origins: &[Origin], frame: Option<&Origin>) -> bool {
    !origin_binding || frame.is_some_and(|o| origins.contains(o))
}

/// Where a browser may receive an item's values, or the reason it may not. Developer items
/// (API keys, environment variables) go only into frames of their listed origins, and with
/// no origin listed nowhere (R-36): a prompt-injected agent must not be able to type a key
/// into an input on the attacker's page. Other items follow their origin binding.
pub fn allows_browser(
    domain: Domain,
    origin_binding: bool,
    origins: &[Origin],
    frame: Option<&Origin>,
) -> Result<(), &'static str> {
    if domain == Domain::Developer {
        if origins.is_empty() {
            return Err("no-origin");
        }
        return allows_frame(true, origins, frame)
            .then_some(())
            .ok_or("origin");
    }
    allows_frame(origin_binding, origins, frame)
        .then_some(())
        .ok_or("origin")
}

/// `keyharbor run` hands values to a process, which can send them anywhere; an origin-bound
/// item's binding would mean nothing there, so such items are never injected.
pub fn allows_process(origin_binding: bool) -> bool {
    !origin_binding
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bound_items_need_an_exact_origin() {
        let github = Origin::parse("https://github.com").unwrap();
        let gist = Origin::parse("https://gist.github.com").unwrap();
        let origins = [github.clone()];
        assert!(allows_frame(true, &origins, Some(&github)));
        assert!(!allows_frame(true, &origins, Some(&gist)));
        assert!(!allows_frame(true, &origins, None));
        assert!(allows_frame(false, &origins, None));
        assert!(allows_process(false));
        let d = Domain::Developer;
        assert_eq!(
            allows_browser(d, false, &[], Some(&github)),
            Err("no-origin")
        );
        assert_eq!(
            allows_browser(d, false, &origins, Some(&gist)),
            Err("origin")
        );
        assert_eq!(allows_browser(d, false, &origins, Some(&github)), Ok(()));
        assert_eq!(
            allows_browser(Domain::Personal, false, &[], Some(&gist)),
            Ok(())
        );
        assert_eq!(
            allows_browser(Domain::Payment, true, &origins, Some(&gist)),
            Err("origin")
        );
        assert!(!allows_process(true));
    }
}
