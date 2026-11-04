//! A value parser over a registry: the accepted names come from the registry (so `--help`
//! and errors list them), and the chosen name becomes the registry's value. A new kind or
//! domain needs no change here (J-185).

use clap::builder::{PossibleValuesParser, TypedValueParser};

/// Accepts exactly `names` and turns the chosen one into `T` with `parse`.
pub fn choice<T>(
    names: impl IntoIterator<Item = &'static str>,
    parse: fn(&str) -> Option<T>,
) -> impl TypedValueParser<Value = T>
where
    T: Clone + Send + Sync + 'static,
{
    PossibleValuesParser::new(names)
        .try_map(move |name| parse(&name).ok_or_else(|| format!("unknown value {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command};
    use keyharbor_domain::vault::{Domain, ItemKind, KINDS};

    fn command() -> Command {
        Command::new("t").arg(
            Arg::new("kind")
                .long("kind")
                .value_parser(choice(KINDS.iter().map(|s| s.name), ItemKind::parse)),
        )
    }

    #[test]
    fn a_registered_name_becomes_its_value() {
        let matches = command()
            .try_get_matches_from(["t", "--kind", "card"])
            .unwrap();
        assert_eq!(matches.get_one::<ItemKind>("kind"), Some(&ItemKind::Card));
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_registered_names() {
        let error = command()
            .try_get_matches_from(["t", "--kind", "bogus"])
            .err()
            .unwrap()
            .to_string();
        let names: Vec<_> = KINDS.iter().map(|s| s.name).collect();
        assert!(error.contains(&names.join(", ")), "{error}");
    }

    #[test]
    fn every_listed_name_parses_to_its_value() {
        let parser = choice(Domain::ALL.map(Domain::as_str), Domain::parse);
        let command = Command::new("t");
        for domain in Domain::ALL {
            let value = parser
                .parse_ref(&command, None, std::ffi::OsStr::new(domain.as_str()))
                .unwrap();
            assert_eq!(value, domain);
        }
    }
}
