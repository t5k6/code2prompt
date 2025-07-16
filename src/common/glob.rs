use anyhow::Result;
use glob::Pattern;

use globset::{Glob, GlobSet, GlobSetBuilder};

pub fn build_globset(patterns: &[Pattern]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p.as_str())?);
    }
    Ok(b.build()?)
}
