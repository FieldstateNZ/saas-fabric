//! What a callback has to present to be believed.
//!
//! These are the tests the brief's authority section is actually about: an
//! anonymous callback must not be able to establish an integration, and a
//! callback must not be usable twice or by the wrong leg of the flow.

use super::flow::FLOW_LIFETIME_SECONDS;
use super::*;

const NOW: u64 = 1_000_000;

fn started(flows: &PendingFlows, step: FlowStep) -> String {
    flows.begin("brett", step, NOW).expect("a flow must start")
}

#[test]
fn a_token_names_the_operator_who_started_the_flow() {
    let flows = PendingFlows::new();
    let token = started(&flows, FlowStep::Creation);

    let flow = flows
        .consume(&token, FlowStep::Creation, NOW)
        .expect("the token must be accepted");

    assert_eq!(flow.operator, "brett");
}

#[test]
fn a_token_cannot_be_spent_twice() {
    // The whole reason this is held server-side rather than signed: a captured
    // callback URL must not be replayable at all, not merely for ten minutes.
    let flows = PendingFlows::new();
    let token = started(&flows, FlowStep::Creation);

    assert!(flows.consume(&token, FlowStep::Creation, NOW).is_some());
    assert!(
        flows.consume(&token, FlowStep::Creation, NOW).is_none(),
        "a second presentation of the same token is a replay"
    );
}

#[test]
fn a_token_issued_for_one_leg_is_refused_by_the_other() {
    // The two callbacks do different things with different inputs. A token
    // that worked at either would let a half-finished flow be steered into the
    // other.
    let flows = PendingFlows::new();
    let token = started(&flows, FlowStep::Creation);

    assert!(flows.consume(&token, FlowStep::Installation, NOW).is_none());
}

#[test]
fn spending_a_token_at_the_wrong_leg_still_spends_it() {
    // Refusing without removing would leave a token an attacker could probe
    // against both legs until one accepted it.
    let flows = PendingFlows::new();
    let token = started(&flows, FlowStep::Creation);

    assert!(flows.consume(&token, FlowStep::Installation, NOW).is_none());
    assert!(
        flows.consume(&token, FlowStep::Creation, NOW).is_none(),
        "a probed token must not remain usable"
    );
}

#[test]
fn a_token_expires() {
    let flows = PendingFlows::new();
    let token = started(&flows, FlowStep::Creation);

    assert!(flows
        .consume(&token, FlowStep::Creation, NOW + FLOW_LIFETIME_SECONDS + 1)
        .is_none());
}

#[test]
fn a_token_this_platform_never_issued_is_refused() {
    let flows = PendingFlows::new();

    assert!(flows
        .consume("a-token-from-somewhere-else", FlowStep::Creation, NOW)
        .is_none());
}

#[test]
fn tokens_are_not_guessable_and_not_repeated() {
    let flows = PendingFlows::new();

    let first = started(&flows, FlowStep::Creation);
    let second = started(&flows, FlowStep::Creation);

    assert_ne!(first, second);
    assert!(first.len() >= 43, "32 random bytes, base64url-encoded: {first}");
}

#[test]
fn starting_a_flow_clears_out_expired_ones() {
    // Nothing else sweeps this map, so an abandoned flow must not accumulate
    // for the life of the process.
    let flows = PendingFlows::new();
    started(&flows, FlowStep::Creation);
    started(&flows, FlowStep::Creation);
    assert_eq!(flows.outstanding(), 2);

    flows
        .begin("brett", FlowStep::Creation, NOW + FLOW_LIFETIME_SECONDS + 1)
        .expect("a flow must start");

    assert_eq!(flows.outstanding(), 1);
}
