#[cfg(any(feature = "cache", feature = "tui"))]
use std::io::{Read, Write};
use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow};
#[cfg(any(feature = "cache", feature = "tui"))]
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::common::cache::{CacheFormat, Cacheable};
use crate::common::hash::HashMap;
use crate::engine::utils::RepoCachePath;

const CACHE_VERSION: u32 = 1;

#[derive(Debug)]
pub struct ScanCache {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct CachedMeta {
    pub token_count: usize,
    pub sha256: [u8; 32], // Sha256 produces a 32-byte hash
}

impl ScanCache {
    /// Opens a connection to the cache DB for a given repository root.
    /// Creates and initializes the DB if needed.
    pub fn open(repo_root: &Path) -> Result<Self> {
        let cache_path =
            RepoCachePath::new(repo_root)?.get_cache_file_path("scan_cache", "sqlite")?;

        let conn = Connection::open(&cache_path).with_context(|| {
            format!("Failed to open cache database at {}", cache_path.display())
        })?;

        // Enable Write-Ahead Logging for better concurrency and performance.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_cache (
                 path TEXT PRIMARY KEY,
                 mtime_nanos INTEGER NOT NULL,
                 size_bytes  INTEGER NOT NULL,
                 sha256  BLOB NOT NULL,
                 token_count INTEGER NOT NULL,
                 content BLOB,
                 cache_version INTEGER NOT NULL
             );",
        )?;

        Ok(Self { conn })
    }

    /// Looks up a file in the cache using its path, modification time, and size.
    pub fn lookup(
        &self,
        rel_path: &str,
        mtime: SystemTime,
        size: u64,
    ) -> Result<Option<CachedMeta>> {
        let mtime_nanos = mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_nanos() as i64;

        let res = self
            .conn
            .query_row(
                "SELECT token_count, sha256 FROM file_cache
                 WHERE path = ?1 AND mtime_nanos = ?2 AND size_bytes = ?3",
                params![rel_path, mtime_nanos, size as i64],
                |row| {
                    let sha_vec: Vec<u8> = row.get(1)?;
                    let sha_array: [u8; 32] = match sha_vec.try_into() {
                        Ok(arr) => arr,
                        Err(v) => {
                            // Log the error if the feature is enabled
                            #[cfg(feature = "logging")]
                            log::warn!(
                                "Failed to convert SHA256 from DB for path '{}'. Vec length: {}. Expected 32.",
                                rel_path,
                                v.len()
                            );
                            // This indicates a corrupted cache entry, so we should treat it as a cache miss.
                            // By returning an error here, .optional() will convert it to Ok(None)
                            return Err(rusqlite::Error::InvalidColumnType(
                                1,
                                "Invalid SHA256 blob length".to_string(),
                                rusqlite::types::Type::Blob,
                            ));
                        }
                    };

                    Ok(CachedMeta {
                        token_count: row.get(0)?,
                        sha256: sha_array,
                    })
                }
            )
            .optional()?; // .optional() gracefully handles no rows found

        Ok(res)
    }

    /// Inserts or updates a file's metadata in the cache.
    pub fn insert(
        &self,
        rel_path: &str,
        mtime: SystemTime,
        size: u64,
        sha256: [u8; 32],
        tokens: usize,
        content: Option<&str>,
    ) -> Result<()> {
        let mtime_nanos = mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_nanos() as i64;

        let compressed_content = content
            .map(|s| {
                let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
                encoder.write_all(s.as_bytes())?;
                encoder.finish()
            })
            .transpose()?;

        self.conn.execute(
            "INSERT OR REPLACE INTO file_cache (path, mtime_nanos, size_bytes, sha256, token_count, content, cache_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rel_path,
                mtime_nanos,
                size as i64,
                sha256.as_ref(),
                tokens as i64,
                compressed_content,
                CACHE_VERSION,
            ],
        )?;
        Ok(())
    }

    /// Fetches the content for a list of relative paths in a single batch query.
    pub fn get_cached_contents(&self, rel_paths: &[&str]) -> Result<HashMap<String, String>> {
        if rel_paths.is_empty() {
            return Ok(HashMap::default());
        }

        let params_sql = vec!["?"; rel_paths.len()].join(",");
        let sql = format!(
            "SELECT path, content FROM file_cache WHERE path IN ({}) AND content IS NOT NULL AND cache_version = {}",
            params_sql, CACHE_VERSION
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(rel_paths.iter()))?;

        let mut results = HashMap::default();
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let compressed_bytes: Option<Vec<u8>> = row.get(1)?;

            if let Some(bytes) = compressed_bytes {
                let mut decoder = GzDecoder::new(&bytes[..]);
                let mut decompressed_content = String::new();
                if decoder.read_to_string(&mut decompressed_content).is_ok() {
                    results.insert(path, decompressed_content);
                }
            }
        }

        Ok(results)
    }
}

/// A wrapper for template variables to make them `Cacheable`.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct TemplateVariables(pub HashMap<String, String>);

impl Cacheable for TemplateVariables {
    const KEY: &'static str = "vars";
    const FORMAT: CacheFormat = CacheFormat::Toml;
}

pub fn load_vars_from_file(path: &Path) -> Result<HashMap<String, String>> {
    // 1. Get the file extension and convert it to lowercase.
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // 2. Read the file content ONCE.
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read vars file: {}", path.display()))?;

    // 3. Dispatch to the correct parser based on the extension.
    let map: HashMap<String, String> = match extension.as_str() {
        "toml" => toml::from_str(&content)
            .with_context(|| format!("Failed to parse '{}' as TOML", path.display()))?,

        "json" => serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse '{}' as JSON", path.display()))?,

        // Handle unsupported or missing extensions
        _ => {
            return Err(anyhow!(
                "Unsupported file type for --vars-file: '{}'. Please use .toml, .json, or .yaml/.yml",
                path.display()
            ));
        }
    };

    Ok(map)
}
