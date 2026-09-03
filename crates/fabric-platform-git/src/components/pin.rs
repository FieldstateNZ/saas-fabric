//! Where a component's version is written, and by which renderer.

/// One place a component's version is written, and how.
///
/// # The renderer is the variant, not a field beside one
///
/// This is the field that would ruin the design if it grew a general escape. A
/// `jsonPath`, a regex or a YAML pointer here would turn a trusted platform
/// document into an arbitrary repository-edit engine, and Fabric into the
/// deputy that runs it — the same mistake as letting a caller name a file, one
/// level further in.
///
/// Making the renderer the *tag* rather than a field beside the others is what
/// makes an invalid combination unrepresentable instead of merely rejected. A
/// Kustomize pin that names no image does not parse; one that names a chart
/// does not either. There is no state in which a renderer is missing what it
/// needs, so no code downstream has to check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "renderer", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Pin {
    /// An `images:` entry in a Kustomize overlay, carrying a tag and a digest.
    KustomizeImage {
        /// The repository-relative file, which must sit under a managed root.
        path: String,

        /// Which image this file pins.
        ///
        /// Required, because an overlay can pin several: SaaS Fabric's control
        /// plane and console share one, and a pin that did not say which would
        /// have to guess between them.
        image: String,
    },

    /// A chart source's `targetRevision` in an Argo Application.
    #[serde(rename_all = "camelCase")]
    ArgoTargetRevision {
        /// The repository-relative file, which must sit under a managed root.
        path: String,

        /// The chart repository the source must name.
        ///
        /// Both halves of the identity are declared, and both must match.
        /// Matching on the chart name alone is enough for the files this
        /// platform has today and is not enough as a rule: two sources could
        /// name charts of the same name from different repositories, and the
        /// difference between them is which software gets deployed.
        repository: String,

        /// The chart the source must name.
        chart: String,
    },
}

impl Pin {
    /// The file this pin is written in.
    ///
    /// Every variant has one, and every variant's is bounded by the same
    /// rules — so the check that bounds them is written once rather than per
    /// renderer.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::KustomizeImage { path, .. } | Self::ArgoTargetRevision { path, .. } => path,
        }
    }

    /// What this pin writes, for a message an operator reads.
    #[must_use]
    pub const fn describe(&self) -> &'static str {
        match self {
            Self::KustomizeImage { .. } => "a Kustomize image pin",
            Self::ArgoTargetRevision { .. } => "an Argo chart revision",
        }
    }
}
