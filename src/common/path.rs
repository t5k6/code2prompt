use std::path::Path;

/// "foo\\bar" -> "foo/bar"
pub fn to_fwd_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}
