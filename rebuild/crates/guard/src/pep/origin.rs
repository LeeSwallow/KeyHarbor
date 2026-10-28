//! Origin binding (03 §3.8, J-12, J-34): where an item's values may go.

use keyharbor_domain::vault::Origin;

/// An origin-bound item may be typed only into a frame whose origin is exactly one of the
/// item's origins (no subdomain wildcards). Unbound items go anywhere.
pub fn allows_frame(origin_binding: bool, origins: &[Origin], frame: Option<&Origin>) -> bool {
    !origin_binding || frame.is_some_and(|o| origins.contains(o))
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
        assert!(!allows_process(true));
    }
}
