//! `GET /api/user/current`

mod profile;

#[cfg(test)]
mod get_current_user_tests;

use axum::extract::State;
use axum::Json;

use crate::models::CurrentUserResponse;
use crate::operator_tier::OperatorTier;
use crate::{ControlPlaneError, Operator};

/// The signed-in user, in ADR 0024 §3's shape.
///
/// # The same gate as `/api/operator`
///
/// It takes an [`Operator`], so the extractor has verified the bearer before
/// this runs: no bearer is `401 unauthenticated`, a refused one — including a
/// member of the realm without the operator role — is `401
/// operator_refused`, and a `200` means a verified operator and nothing else.
/// The console's gateway probe depends on exactly that (see
/// `apps/control-plane-ui/src/session/gateway.ts`).
///
/// # Presentation only
///
/// The profile is read from the bearer the extractor already verified, by a
/// reader private to this module; nothing else in the crate may read a claim
/// that way. `roles` is the configured tier, not the token's roles, and
/// confers nothing: authority is still the [`Operator`] every route takes.
pub(crate) async fn get_current_user(
    State(tier): State<OperatorTier>,
    operator: Operator,
) -> Result<Json<CurrentUserResponse>, ControlPlaneError> {
    let profile = profile::read(&operator)?;

    Ok(Json(CurrentUserResponse {
        subject: profile.subject,
        user_name: profile.user_name,
        display_name: profile.display_name,
        email: profile.email,
        roles: vec![tier.role().to_owned()],
    }))
}
