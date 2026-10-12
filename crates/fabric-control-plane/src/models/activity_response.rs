//! What the activity feed looks like on the wire.

use fabric_client_model::catalogue::ProductActivity;

/// The most actions one activity response carries.
///
/// The feed is a convenience view of recent operator actions, not the audit
/// trail (ADR 0028), so it is bounded rather than grown with the platform's
/// whole history. 200 is several screens of the console's table; anything
/// older is in the durable audit trail, which is complete.
pub(crate) const ACTIVITY_VIEW_LIMIT: usize = 200;

/// The most recent recorded actions an operator can see, newest first.
///
/// An object rather than a bare array, for the same reason
/// [`ClientListResponse`](super::ClientListResponse) is, and because the
/// bound is part of the answer: `omitted` says how many older actions were
/// left out, so the console never presents a bounded view as everything.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivityResponse {
    /// At most [`ACTIVITY_VIEW_LIMIT`] actions, newest first.
    pub(crate) activity: Vec<ProductActivity>,

    /// The bound this response was cut to.
    pub(crate) limit: usize,

    /// How many older recorded actions this view left out.
    pub(crate) omitted: usize,
}

impl ActivityResponse {
    /// The newest [`ACTIVITY_VIEW_LIMIT`] of `activity`, and how many were
    /// left out. Sorting is done here so the bound always keeps the newest.
    ///
    /// `activity` is in append order within each document, and timestamps
    /// are whole seconds, so many actions can share one. Position breaks the
    /// tie: among equal times the later-appended action is newer, and it is
    /// the oldest of them that the bound drops.
    pub(crate) fn newest(activity: Vec<ProductActivity>) -> Self {
        let mut ordered: Vec<(usize, ProductActivity)> = activity.into_iter().enumerate().collect();
        ordered.sort_by_key(|(appended, action)| std::cmp::Reverse((action.at, *appended)));
        let omitted = ordered.len().saturating_sub(ACTIVITY_VIEW_LIMIT);
        let activity = ordered
            .into_iter()
            .take(ACTIVITY_VIEW_LIMIT)
            .map(|(_, action)| action)
            .collect();

        Self {
            activity,
            limit: ACTIVITY_VIEW_LIMIT,
            omitted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(at: u64) -> ProductActivity {
        ProductActivity {
            at,
            operator: "operator".to_owned(),
            action: "Client configuration updated".to_owned(),
            resource: "acme".to_owned(),
        }
    }

    #[test]
    fn the_view_keeps_the_newest_actions_and_counts_the_rest() {
        let total = ACTIVITY_VIEW_LIMIT + 7;
        let view = ActivityResponse::newest((0..total as u64).map(action).collect());

        assert_eq!(view.activity.len(), ACTIVITY_VIEW_LIMIT);
        assert_eq!(view.omitted, 7);
        assert_eq!(view.activity.first().map(|a| a.at), Some(total as u64 - 1));
        assert_eq!(view.activity.last().map(|a| a.at), Some(7));
    }

    #[test]
    fn among_actions_in_one_second_the_view_keeps_the_last_appended() {
        let total = ACTIVITY_VIEW_LIMIT + 5;
        let actions = (0..total)
            .map(|appended| ProductActivity {
                resource: appended.to_string(),
                ..action(1_000)
            })
            .collect();

        let view = ActivityResponse::newest(actions);

        let kept: Vec<String> = view.activity.iter().map(|a| a.resource.clone()).collect();
        let newest: Vec<String> = (5..total).rev().map(|appended| appended.to_string()).collect();
        assert_eq!(kept, newest);
        assert_eq!(view.omitted, 5);
    }

    #[test]
    fn a_short_history_is_shown_whole() {
        let view = ActivityResponse::newest(vec![action(1), action(2)]);

        assert_eq!(view.activity.len(), 2);
        assert_eq!(view.omitted, 0);
    }
}
