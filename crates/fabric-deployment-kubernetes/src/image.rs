//! A version label is evidence only alongside its running content digest.
use fabric_platform_management::Version;

pub(crate) fn pinned<'a>(image: &'a str, repository: &str) -> Option<(&'a str, &'a str)> {
    let (tagged, digest) = image.split_once('@')?;
    let version = tagged.strip_prefix(repository)?.strip_prefix(':')?;
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Version::parse(version)?;
    Some((version, digest))
}

pub(crate) fn running_digest(image_id: &str, digest: &str) -> bool {
    image_id == digest
        || image_id
            .rsplit_once('@')
            .is_some_and(|(_, actual)| actual == digest)
}
