//! Tier manifest for the Ladybug corpus.
//!
//! Reads `cases/cypher/ladybug/tiers.toml`, which partitions the corpus
//! into `core` (headline conformance), `kuzu-ext` (Kuzu-specific
//! surface, reported separately) and `broken-import` (manual overrides
//! for structurally unpassable imports; most broken imports are
//! machine-tagged at runtime instead).
//!
//! The manifest is a deliberately tiny TOML subset — a `default = "…"`
//! line plus a `[tiers]` table of `"path/prefix" = "tier"` entries — so
//! we hand-parse it instead of pulling in a TOML crate. Keys are path
//! prefixes relative to `cases/cypher/ladybug/` at any granularity
//! (suite dir, subdir, or per-case file); the deepest matching key
//! wins, i.e. a per-case entry overrides its subdir which overrides
//! its suite dir.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Core,
    KuzuExt,
    BrokenImport,
}

impl Tier {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "core" => Some(Self::Core),
            "kuzu-ext" => Some(Self::KuzuExt),
            "broken-import" => Some(Self::BrokenImport),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::KuzuExt => "kuzu-ext",
            Self::BrokenImport => "broken-import",
        }
    }
}

#[derive(Debug)]
pub struct TierManifest {
    default: Tier,
    /// Prefix (relative, `/`-separated, no trailing slash) → tier.
    entries: HashMap<String, Tier>,
}

impl TierManifest {
    /// Load the manifest from disk. A missing file yields an
    /// all-`core` manifest so checkouts without the manifest keep the
    /// old single-number behaviour.
    pub fn load(path: &Path) -> Self {
        let mut manifest = Self {
            default: Tier::Core,
            entries: HashMap::new(),
        };
        let Ok(raw) = std::fs::read_to_string(path) else {
            return manifest;
        };
        let mut in_tiers = false;
        for raw_line in raw.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(section) = line.strip_prefix('[') {
                in_tiers = section.trim_end_matches(']').trim() == "tiers";
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = unquote(key.trim());
            let value = unquote(value.trim());
            let Some(tier) = Tier::parse(&value) else {
                panic!("tiers.toml: unknown tier `{value}` for key `{key}`");
            };
            if in_tiers {
                manifest
                    .entries
                    .insert(key.trim_matches('/').to_string(), tier);
            } else if key == "default" {
                manifest.default = tier;
            }
        }
        manifest
    }

    /// Classify a case by its path relative to the corpus root.
    /// Deepest matching prefix wins; unmatched paths get `default`.
    pub fn lookup(&self, rel_path: &str) -> Tier {
        let rel = rel_path.trim_matches('/');
        let mut best: Option<(usize, Tier)> = None;
        for (prefix, tier) in &self.entries {
            let matches = rel == prefix.as_str()
                || (rel.len() > prefix.len()
                    && rel.starts_with(prefix.as_str())
                    && rel.as_bytes()[prefix.len()] == b'/');
            if matches && best.map(|(len, _)| prefix.len() > len).unwrap_or(true) {
                best = Some((prefix.len(), *tier));
            }
        }
        best.map(|(_, tier)| tier).unwrap_or(self.default)
    }
}

fn strip_comment(line: &str) -> &str {
    // `#` never appears inside manifest keys/values we author, so a
    // plain scan is enough.
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn unquote(text: &str) -> String {
    let t = text.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> TierManifest {
        let mut entries = HashMap::new();
        entries.insert("transaction".to_string(), Tier::KuzuExt);
        entries.insert("function/gds".to_string(), Tier::KuzuExt);
        entries.insert(
            "function/gds/basic/0001_Special_abc.case".to_string(),
            Tier::Core,
        );
        TierManifest {
            default: Tier::Core,
            entries,
        }
    }

    #[test]
    fn deepest_prefix_wins() {
        let m = manifest();
        assert_eq!(m.lookup("match/match1/0001_Foo_ab.case"), Tier::Core);
        assert_eq!(
            m.lookup("transaction/basic/0001_Foo_ab.case"),
            Tier::KuzuExt
        );
        assert_eq!(
            m.lookup("function/gds/basic/0002_Foo_ab.case"),
            Tier::KuzuExt
        );
        assert_eq!(
            m.lookup("function/gds/basic/0001_Special_abc.case"),
            Tier::Core
        );
        // Prefix must match on a component boundary.
        assert_eq!(m.lookup("transactional/0001_Foo_ab.case"), Tier::Core);
    }

    #[test]
    fn real_manifest_loads() {
        let m = TierManifest::load(Path::new("cases/cypher/ladybug/tiers.toml"));
        assert_eq!(m.lookup("transaction/basic/x.case"), Tier::KuzuExt);
        assert_eq!(m.lookup("function/list/x.case"), Tier::Core);
        assert_eq!(m.lookup("function/hash/x.case"), Tier::KuzuExt);
        assert_eq!(m.lookup("tck/match/match1/x.case"), Tier::Core);
    }
}
