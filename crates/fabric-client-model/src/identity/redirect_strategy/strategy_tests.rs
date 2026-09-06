//! Tests for how a redirect strategy is built, spelled, and read back.

use crate::{AppScheme, RedirectStrategy, RedirectStrategyKind, RedirectUri};

/// A callback of each kind, for the variant that admits it.
fn uri(value: &str) -> RedirectUri {
    RedirectUri::try_new(value).unwrap()
}

/// One strategy of every variant, each holding a callback it admits.
fn every_variant() -> Vec<RedirectStrategy> {
    let custom = RedirectStrategyKind::CustomScheme(AppScheme::try_new("nz.fieldstate.slipway").unwrap());

    vec![
        RedirectStrategy::try_new(
            RedirectStrategyKind::ClaimedHttps,
            vec![uri("https://www.example.com/callback")],
        )
        .unwrap(),
        RedirectStrategy::try_new(
            RedirectStrategyKind::PrivateNetwork,
            vec![uri("http://acme.lucentroot.internal/callback")],
        )
        .unwrap(),
        RedirectStrategy::try_new(
            RedirectStrategyKind::Development,
            vec![uri("http://127.0.0.1:*/callback")],
        )
        .unwrap(),
        RedirectStrategy::try_new(custom, vec![uri("nz.fieldstate.slipway:/callback")]).unwrap(),
    ]
}

#[test]
fn a_redirect_strategy_survives_a_serialise_and_parse_round_trip() {
    // Both directions, over every variant. The serialise half is easy to miss
    // and is not optional: the control-plane API renders application clients
    // straight out of this type, and a strategy that only reads would not
    // compile there.
    for strategy in every_variant() {
        let rendered = serde_norway::to_string(&strategy).unwrap();
        let reread: RedirectStrategy = serde_norway::from_str(&rendered).unwrap();

        assert_eq!(reread, strategy, "{rendered}");
    }
}

#[test]
fn the_document_spells_the_strategy_in_camel_case_beside_its_uris() {
    let rendered = serde_norway::to_string(&every_variant()[0]).unwrap();

    assert!(rendered.contains("strategy: claimedHttps"), "{rendered}");
    assert!(
        rendered.contains("- https://www.example.com/callback"),
        "{rendered}"
    );
}

#[test]
fn a_misspelled_key_inside_a_redirect_block_is_refused_rather_than_ignored() {
    let text = "strategy: claimedHttps\nuris:\n  - https://www.example.com/cb\nurls: []\n";

    assert!(serde_norway::from_str::<RedirectStrategy>(text).is_err());
}

#[test]
fn a_strategy_that_is_not_one_of_the_four_is_refused() {
    let text = "strategy: anything\nuris:\n  - https://www.example.com/cb\n";

    assert!(serde_norway::from_str::<RedirectStrategy>(text).is_err());
}

#[test]
fn the_constructor_refuses_a_callback_the_strategy_does_not_admit() {
    // The reason the fields are private: kind and uris only mean anything
    // together, and a public field would let any caller assemble the exact
    // ambiguity this type exists to remove.
    let refused = RedirectStrategy::try_new(
        RedirectStrategyKind::ClaimedHttps,
        vec![uri("http://localhost:5173/callback")],
    );

    assert!(refused.is_err());
}
