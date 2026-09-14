//! How an update's payload is shaped for the procedure that receives it.

/// The JSON shape a mapping's payload argument expects.
///
/// # Why this exists
///
/// Every mapping observed before `ndc-postgres`'s keyed procedures sent a
/// payload as `{col: value}` — the row itself, one field per key. A real
/// `update_articles_by_id_and_tenant_key`'s `update_columns` argument does not
/// take that: it wants a per-column *operation*, `{col: {"_set": value}}`, so
/// the same argument position could in principle carry `_increment` or
/// `_append` instead. Nothing in this crate builds anything but `_set`, but
/// the wrapping still has to be there or the connector cannot parse the
/// payload at all.
///
/// A closed enum rather than a free-form template: the two shapes below are
/// the only ones observed, and a mapping that needs a third should add one
/// here rather than smuggle it in as a string to interpolate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadShape {
    /// `{col: value}` — the row's fields, sent as-is. What every mapping sent
    /// before keyed procedures existed, and the only shape an insert's
    /// `objects` argument is ever observed to take.
    #[default]
    Values,

    /// `{col: {"_set": value}}` — what `ndc-postgres`'s `update_columns`
    /// argument takes on a keyed update procedure.
    ///
    /// Only an update mapping may declare this: an insert sends an array of
    /// row objects, never a per-column operation map, and a delete has no
    /// payload at all. Config validation refuses both.
    SetOperations,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shape_is_values() {
        assert_eq!(PayloadShape::default(), PayloadShape::Values);
    }

    #[test]
    fn parses_snake_case_from_configuration() {
        let shape: PayloadShape = serde_json::from_str(r#""set_operations""#).unwrap();

        assert_eq!(shape, PayloadShape::SetOperations);
    }
}
