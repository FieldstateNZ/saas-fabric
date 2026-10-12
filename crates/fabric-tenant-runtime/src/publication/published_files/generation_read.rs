//! One read of the published files: the record and the three documents it
//! names.

use super::PublishedFiles;
use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::file_reads::{read, read_optional};
use crate::publication::PublishedStateSource as _;
use crate::SourceError;

impl PublishedFiles {
    /// One read of the record and the three documents it names, each
    /// checked against its digest. `Ok(None)` when there is no record.
    pub(super) async fn read(&self) -> Result<Option<CompleteGeneration>, SourceError> {
        let origin = self.describe();
        let Some(record) = read_optional(&self.record, &origin).await? else {
            return Ok(None);
        };
        let data_sources = read(&self.data_sources, &origin).await?;
        let tenants = read(&self.tenants, &origin).await?;
        let catalog = read(&self.catalog, &origin).await?;
        CompleteGeneration::verify(record, data_sources, tenants, catalog, &origin).map(Some)
    }
}
