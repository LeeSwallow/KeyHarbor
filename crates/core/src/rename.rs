//! Renaming or moving an item (`github` → `work/github`, 09 §9.4). The values stay sealed to
//! the item's id, so only the name and the metadata MAC change. Names are also keys elsewhere:
//! a policy rule on `item <name>` would silently stop applying, which is a weakening, so a
//! rename is refused while one names the item (J-104). Placeholders that agents hold
//! (`{{secret:github.password}}`) stop resolving; the interfaces warn before asking.

use crate::Vault;
use anyhow::{Result, bail};
use keyharbor_domain::{policy::Resource, ports::PolicyStore, vault::ItemName};

pub fn rename_item(
    policies: &dyn PolicyStore,
    vault: &Vault,
    from: &ItemName,
    to: &ItemName,
) -> Result<()> {
    let rules = policies
        .policy_rules()?
        .into_iter()
        .filter(|r| r.resource == Resource::Item && r.target == from.as_str())
        .count();
    if rules > 0 {
        bail!("{rules} policy rule(s) name this item; change them first, then rename");
    }
    vault.rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::rename_item;
    use crate::tests::{env, init, login, open_store, pref};
    use crate::{ItemStatus, NewItem};
    use keyharbor_domain::{
        policy::{Effect, Resource, Rule},
        ports::PolicyStore,
        vault::ItemName,
    };

    fn name(n: &str) -> ItemName {
        ItemName::parse(n).unwrap()
    }

    #[test]
    fn a_rename_keeps_the_values_and_frees_the_old_name() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw-1")).unwrap();
        rename_item(
            &open_store(&e.data),
            &v,
            &name("github"),
            &name("work/github"),
        )
        .unwrap();
        assert_eq!(
            &*v.resolve(&pref("work/github", "password")).unwrap(),
            "pw-1"
        );
        assert!(
            v.resolve(&pref("github", "password")).is_err(),
            "the old placeholder no longer resolves"
        );
        let names: Vec<String> = v.list().unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(names, ["work/github"]);
        assert!(
            !v.list().unwrap()[0].tampered,
            "the metadata MAC covers the new name"
        );
    }

    #[test]
    fn a_rename_is_refused_while_a_rule_names_the_item() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let store = open_store(&e.data);
        store
            .policy_set(&Rule::new("codex", Resource::Item, "github", Effect::Deny).unwrap())
            .unwrap();
        let err = rename_item(&store, &v, &name("github"), &name("gh"))
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "1 policy rule(s) name this item; change them first, then rename"
        );
        assert_eq!(v.list().unwrap()[0].name, "github");
    }

    #[test]
    fn a_rename_is_refused_onto_a_taken_name_from_a_draft_and_while_drafts_wait() {
        let e = env();
        let v = init(&e);
        let store = open_store(&e.data);
        v.add(login("github", "a")).unwrap();
        v.add(login("gitlab", "b")).unwrap();
        let taken = rename_item(&store, &v, &name("github"), &name("gitlab"))
            .unwrap_err()
            .to_string();
        assert_eq!(taken, "an item named gitlab already exists");
        v.add(NewItem {
            status: ItemStatus::Draft,
            ..login("github~draft-1", "c")
        })
        .unwrap();
        let waiting = rename_item(&store, &v, &name("github"), &name("gh"))
            .unwrap_err()
            .to_string();
        assert_eq!(
            waiting,
            "drafts of github wait to be confirmed or discarded; settle them first"
        );
        let draft = rename_item(&store, &v, &name("github~draft-1"), &name("x"))
            .unwrap_err()
            .to_string();
        assert_eq!(draft, "a draft is renamed by confirming it");
        assert!(rename_item(&store, &v, &name("nope"), &name("x")).is_err());
    }
}
