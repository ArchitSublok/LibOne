//! Phase 3 Local Suggestion Engine
//!
//! Analyzes indexed document metadata locally using deterministic, explainable
//! heuristics. Operates strictly on:
//! - filenames and extensions
//! - folder paths
//! - timestamps
//! - file sizes
//! - file identities (NTFS volume + index) / hashes
//! - existing collections and tags
//!
//! Never accesses document contents or cloud AI services.
//! Produces pending suggestions that require explicit user approval.

use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};

use crate::models::{Collection, IndexedFile, Tag};

/// Structure representing a suggested organization action before persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSuggestion {
    pub suggestion_type: String, // "suggest_collection", "suggest_tag", "likely_version", "stale_file"
    pub title: String,
    pub rationale: String,
    pub confidence: f64,
    pub affected_file_ids: Vec<i64>,
    pub suggested_tag_id: Option<i64>,
    pub suggested_tag_name: Option<String>,
    pub suggested_collection_id: Option<i64>,
    pub suggested_collection_name: Option<String>,
}

/// Structure representing a detected duplicate group before persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct RawDuplicateGroup {
    pub match_type: String, // "file_identity", "exact_hash", "name_and_size"
    pub confidence: f64,
    pub file_ids: Vec<i64>,
}

/// Configuration options for the suggestion engine.
#[derive(Debug, Clone)]
pub struct SuggestionOptions {
    /// Number of days of inactivity before a file is considered stale (default 180).
    pub stale_days_threshold: i64,
    /// Minimum files required to propose a folder-based collection (default 3).
    pub min_collection_cluster_size: usize,
    /// Minimum files required to propose a version series (default 2).
    pub min_version_cluster_size: usize,
}

impl Default for SuggestionOptions {
    fn default() -> Self {
        Self {
            stale_days_threshold: 180,
            min_collection_cluster_size: 3,
            min_version_cluster_size: 2,
        }
    }
}

/// The local suggestion engine.
pub struct SuggestionEngine {
    options: SuggestionOptions,
}

impl Default for SuggestionEngine {
    fn default() -> Self {
        Self::new(SuggestionOptions::default())
    }
}

impl SuggestionEngine {
    pub fn new(options: SuggestionOptions) -> Self {
        Self { options }
    }

    /// Evaluates all indexed files and existing metadata to generate duplicate groups.
    pub fn find_duplicates(&self, files: &[IndexedFile]) -> Vec<RawDuplicateGroup> {
        let mut groups = Vec::new();
        let mut claimed_file_ids = HashSet::new();

        // 1. Exact match using Windows NTFS File Identity (vol:file_id)
        // If two non-missing indexed files share the same NTFS file identity, they are the same physical file (hardlink/NTFS identity collision).
        let mut identity_map: HashMap<&str, Vec<&IndexedFile>> = HashMap::new();
        for file in files {
            if file.status != "missing" {
                if let Some(ref fid) = file.file_id_nullable {
                    if !fid.trim().is_empty() {
                        identity_map.entry(fid.as_str()).or_default().push(file);
                    }
                }
            }
        }

        for (_fid, group) in identity_map {
            if group.len() >= 2 {
                let ids: Vec<i64> = group.iter().map(|f| f.id).collect();
                for &id in &ids {
                    claimed_file_ids.insert(id);
                }
                groups.push(RawDuplicateGroup {
                    match_type: "file_identity".to_string(),
                    confidence: 1.0,
                    file_ids: ids,
                });
            }
        }

        // 2. Exact match using computed content hash + size
        let mut hash_map: HashMap<(&str, i64), Vec<&IndexedFile>> = HashMap::new();
        for file in files {
            if file.status != "missing"
                && file.size_bytes > 0
                && !claimed_file_ids.contains(&file.id)
            {
                if let Some(ref h) = file.content_hash_nullable {
                    if !h.trim().is_empty() {
                        hash_map
                            .entry((h.as_str(), file.size_bytes))
                            .or_default()
                            .push(file);
                    }
                }
            }
        }

        for ((_hash, _size), group) in hash_map {
            if group.len() >= 2 {
                let ids: Vec<i64> = group.iter().map(|f| f.id).collect();
                for &id in &ids {
                    claimed_file_ids.insert(id);
                }
                groups.push(RawDuplicateGroup {
                    match_type: "exact_hash".to_string(),
                    confidence: 1.0,
                    file_ids: ids,
                });
            }
        }

        // 3. Name & Size match (heuristic duplicate candidate when hashes aren't both computed)
        let mut name_size_map: HashMap<(String, i64), Vec<&IndexedFile>> = HashMap::new();
        for file in files {
            if file.status != "missing"
                && file.size_bytes > 0
                && !claimed_file_ids.contains(&file.id)
            {
                let norm_name = file.file_name.trim().to_lowercase();
                name_size_map
                    .entry((norm_name, file.size_bytes))
                    .or_default()
                    .push(file);
            }
        }

        for ((_name, _size), group) in name_size_map {
            if group.len() >= 2 {
                let ids: Vec<i64> = group.iter().map(|f| f.id).collect();
                for &id in &ids {
                    claimed_file_ids.insert(id);
                }
                groups.push(RawDuplicateGroup {
                    match_type: "name_and_size".to_string(),
                    confidence: 0.85,
                    file_ids: ids,
                });
            }
        }

        // Deterministic sort: highest confidence first, then by earliest file ID
        groups.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.file_ids.first().cmp(&b.file_ids.first()))
        });

        groups
    }

    /// Evaluates likely versions: similar filenames such as "report", "report-final", "report-v2".
    pub fn find_likely_versions(&self, files: &[IndexedFile]) -> Vec<RawSuggestion> {
        let mut suggestions = Vec::new();

        // Group files by containing directory and normalized base stem
        // A stem is the filename without extension, stripped of version qualifiers.
        let mut dir_stem_groups: HashMap<(String, String), Vec<&IndexedFile>> = HashMap::new();

        for file in files {
            if file.status == "missing" {
                continue;
            }
            let parent = get_parent_directory(&file.normalized_path);
            let stem = get_filename_stem(&file.file_name);
            let normalized_base = extract_version_base_stem(&stem);

            if !normalized_base.is_empty() {
                dir_stem_groups
                    .entry((parent, normalized_base))
                    .or_default()
                    .push(file);
            }
        }

        for ((_dir, base_stem), group) in dir_stem_groups {
            if group.len() >= self.options.min_version_cluster_size {
                // Ensure there is actually variation in the filenames
                let distinct_names: HashSet<&str> =
                    group.iter().map(|f| f.file_name.as_str()).collect();
                if distinct_names.len() >= 2 {
                    let mut file_ids: Vec<i64> = group.iter().map(|f| f.id).collect();
                    file_ids.sort_unstable();

                    let file_names_preview: Vec<&str> =
                        group.iter().map(|f| f.file_name.as_str()).collect();
                    let preview_str = if file_names_preview.len() <= 3 {
                        file_names_preview.join(", ")
                    } else {
                        format!(
                            "{}, and {} more",
                            file_names_preview[..2].join(", "),
                            file_names_preview.len() - 2
                        )
                    };

                    suggestions.push(RawSuggestion {
                        suggestion_type: "likely_version".to_string(),
                        title: format!("Version sequence detected: '{base_stem}'"),
                        rationale: format!(
                            "Detected {} related versions in the same folder ({})",
                            group.len(),
                            preview_str
                        ),
                        confidence: 0.85,
                        affected_file_ids: file_ids,
                        suggested_tag_id: None,
                        suggested_tag_name: Some("version-series".to_string()),
                        suggested_collection_id: None,
                        suggested_collection_name: None,
                    });
                }
            }
        }

        suggestions.sort_by(|a, b| a.title.cmp(&b.title));
        suggestions
    }

    /// Evaluates suggested collections based on folder clusters and project topics.
    pub fn find_suggested_collections(
        &self,
        files: &[IndexedFile],
        existing_collections: &[Collection],
        file_collection_map: &HashMap<i64, Vec<i64>>,
    ) -> Vec<RawSuggestion> {
        let mut suggestions = Vec::new();

        // 1. Group files by immediate parent folder
        let mut folder_files: HashMap<String, Vec<&IndexedFile>> = HashMap::new();
        for file in files {
            if file.status == "missing" {
                continue;
            }
            let parent = get_parent_directory(&file.normalized_path);
            if !parent.is_empty() {
                folder_files.entry(parent).or_default().push(file);
            }
        }

        // Map existing collection names lowercase -> Collection
        let existing_by_name: HashMap<String, &Collection> = existing_collections
            .iter()
            .map(|c| (c.name.trim().to_lowercase(), c))
            .collect();

        for (folder_path, group) in folder_files {
            if group.len() >= self.options.min_collection_cluster_size {
                let folder_name = get_last_path_component(&folder_path);
                if folder_name.is_empty() || is_generic_directory(&folder_name) {
                    continue;
                }

                // Check how many files in this group already have this collection (or any collection)
                let existing_col = existing_by_name.get(&folder_name.to_lowercase());
                let (target_col_id, target_col_name) = match existing_col {
                    Some(c) => (Some(c.id), c.name.clone()),
                    None => (None, to_title_case(&folder_name)),
                };

                // Find files that do NOT already belong to target_col_id
                let unlinked_files: Vec<&IndexedFile> = group
                    .iter()
                    .filter(|f| {
                        if let Some(col_id) = target_col_id {
                            if let Some(cols) = file_collection_map.get(&f.id) {
                                !cols.contains(&col_id)
                            } else {
                                true
                            }
                        } else {
                            // If it's a new collection, include files that have 0 collections or at least the whole folder
                            true
                        }
                    })
                    .copied()
                    .collect();

                if unlinked_files.len() >= self.options.min_collection_cluster_size {
                    let mut file_ids: Vec<i64> = unlinked_files.iter().map(|f| f.id).collect();
                    file_ids.sort_unstable();

                    let rationale = if target_col_id.is_some() {
                        format!(
                            "Found {} unorganized files in folder '{}' matching existing collection '{}'",
                            unlinked_files.len(),
                            folder_name,
                            target_col_name
                        )
                    } else {
                        format!(
                            "Folder '{}' contains {} documents. Consider organizing them into a collection.",
                            folder_name,
                            unlinked_files.len()
                        )
                    };

                    suggestions.push(RawSuggestion {
                        suggestion_type: "suggest_collection".to_string(),
                        title: format!(
                            "Group files in '{folder_name}' into collection '{target_col_name}'"
                        ),
                        rationale,
                        confidence: 0.80,
                        affected_file_ids: file_ids,
                        suggested_tag_id: None,
                        suggested_tag_name: None,
                        suggested_collection_id: target_col_id,
                        suggested_collection_name: Some(target_col_name),
                    });
                }
            }
        }

        suggestions.sort_by(|a, b| b.affected_file_ids.len().cmp(&a.affected_file_ids.len()));
        suggestions
    }

    /// Evaluates suggested tags based on:
    /// - File format / extension categories (e.g. PDF, spreadsheet, document)
    /// - Year tokens in filename or directory (e.g. "2024", "2025")
    /// - Common semantic keywords in filenames (e.g. "invoice", "receipt", "contract", "tax")
    pub fn find_suggested_tags(
        &self,
        files: &[IndexedFile],
        existing_tags: &[Tag],
        file_tag_map: &HashMap<i64, Vec<i64>>,
    ) -> Vec<RawSuggestion> {
        let mut suggestions = Vec::new();

        // Existing tags lookup
        let existing_by_name: HashMap<String, &Tag> = existing_tags
            .iter()
            .map(|t| (t.name.trim().to_lowercase(), t))
            .collect();

        // Group files by (suggested_tag_name, is_year_or_format_or_keyword)
        let mut tag_to_files: HashMap<String, Vec<&IndexedFile>> = HashMap::new();

        for file in files {
            if file.status == "missing" {
                continue;
            }

            let mut candidates = HashSet::new();

            // 1. Year token extraction from filename or path (1990..=2050)
            if let Some(year) =
                extract_year(&file.file_name).or_else(|| extract_year(&file.normalized_path))
            {
                candidates.insert(year);
            }

            // 2. Semantic workflow keywords in filename
            let stem = get_filename_stem(&file.file_name);
            let tokens = tokenize_words(&stem);
            for token in tokens {
                if is_common_document_keyword(&token) {
                    candidates.insert(token);
                }
            }

            // 3. Extension group tags for non-generic files
            if let Some(ext_group) = get_extension_category(&file.extension) {
                candidates.insert(ext_group.to_string());
            }

            // Check against existing assigned tags for this file
            let assigned_tag_ids = file_tag_map.get(&file.id);

            for cand in candidates {
                let norm_cand = cand.to_lowercase();
                let already_has_tag = if let Some(existing) = existing_by_name.get(&norm_cand) {
                    assigned_tag_ids.map_or(false, |tags| tags.contains(&existing.id))
                } else {
                    false
                };

                if !already_has_tag {
                    tag_to_files.entry(cand).or_default().push(file);
                }
            }
        }

        for (tag_name, group) in tag_to_files {
            // Only suggest tags that apply to at least 2 files or match an existing tag
            let existing_tag = existing_by_name.get(&tag_name.to_lowercase());
            let min_size = if existing_tag.is_some() { 1 } else { 2 };

            if group.len() >= min_size {
                let mut file_ids: Vec<i64> = group.iter().map(|f| f.id).collect();
                file_ids.sort_unstable();

                let (target_tag_id, display_name) = match existing_tag {
                    Some(t) => (Some(t.id), t.name.clone()),
                    None => (None, tag_name.clone()),
                };

                let rationale = if target_tag_id.is_some() {
                    format!(
                        "{} files match existing tag '{}' based on filename or format metadata",
                        file_ids.len(),
                        display_name
                    )
                } else {
                    format!(
                        "Suggested tag '{}' discovered from filename patterns across {} documents",
                        display_name,
                        file_ids.len()
                    )
                };

                suggestions.push(RawSuggestion {
                    suggestion_type: "suggest_tag".to_string(),
                    title: format!("Tag {} files with '{}'", file_ids.len(), display_name),
                    rationale,
                    confidence: if target_tag_id.is_some() { 0.90 } else { 0.75 },
                    affected_file_ids: file_ids,
                    suggested_tag_id: target_tag_id,
                    suggested_tag_name: Some(display_name),
                    suggested_collection_id: None,
                    suggested_collection_name: None,
                });
            }
        }

        suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        suggestions
    }

    /// Evaluates stale files:
    /// Old files not seen or modified for a configurable period (default 180 days).
    pub fn find_stale_files(
        &self,
        files: &[IndexedFile],
        now: DateTime<Utc>,
    ) -> Vec<RawSuggestion> {
        let mut suggestions = Vec::new();
        let cutoff = now - Duration::days(self.options.stale_days_threshold);

        let mut stale_files = Vec::new();

        for file in files {
            // A file is stale if:
            // 1. It is marked "missing"
            // OR
            // 2. Its modified_at timestamp is older than cutoff AND last_seen_at is older than cutoff (or unrefreshed)
            let mut is_stale = false;
            let mut reason = String::new();

            if file.status == "missing" {
                if let Ok(last_seen) = DateTime::parse_from_rfc3339(&file.last_seen_at) {
                    if last_seen.with_timezone(&Utc) < cutoff {
                        is_stale = true;
                        reason = format!(
                            "Missing from disk since {}",
                            file.last_seen_at.chars().take(10).collect::<String>()
                        );
                    }
                } else {
                    is_stale = true;
                    reason = "Missing from disk with unrecorded date".to_string();
                }
            } else if let Some(ref mod_at) = file.modified_at {
                if let Ok(mod_dt) = DateTime::parse_from_rfc3339(mod_at) {
                    if mod_dt.with_timezone(&Utc) < cutoff {
                        is_stale = true;
                        reason = format!(
                            "Unmodified for over {} days (last modified: {})",
                            self.options.stale_days_threshold,
                            mod_at.chars().take(10).collect::<String>()
                        );
                    }
                }
            }

            if is_stale {
                stale_files.push((file, reason));
            }
        }

        if !stale_files.is_empty() {
            // Create a grouped cleanup suggestion for missing files
            let missing_files: Vec<&IndexedFile> = stale_files
                .iter()
                .filter(|(f, _)| f.status == "missing")
                .map(|(f, _)| *f)
                .collect();

            if !missing_files.is_empty() {
                let mut ids: Vec<i64> = missing_files.iter().map(|f| f.id).collect();
                ids.sort_unstable();

                suggestions.push(RawSuggestion {
                    suggestion_type: "stale_file".to_string(),
                    title: format!("Clean up {} stale missing index records", ids.len()),
                    rationale: format!(
                        "{} files have been missing from disk for over {} days. Cleaning removes index records from LibOne without affecting your disk.",
                        ids.len(),
                        self.options.stale_days_threshold
                    ),
                    confidence: 0.95,
                    affected_file_ids: ids,
                    suggested_tag_id: None,
                    suggested_tag_name: Some("stale-missing".to_string()),
                    suggested_collection_id: None,
                    suggested_collection_name: None,
                });
            }

            // Also suggest archiving old unmodified files
            let unmodified_files: Vec<&IndexedFile> = stale_files
                .iter()
                .filter(|(f, _)| f.status != "missing")
                .map(|(f, _)| *f)
                .collect();

            if !unmodified_files.is_empty() {
                let mut ids: Vec<i64> = unmodified_files.iter().map(|f| f.id).collect();
                ids.sort_unstable();

                suggestions.push(RawSuggestion {
                    suggestion_type: "stale_file".to_string(),
                    title: format!("Archive candidate: {} inactive documents", ids.len()),
                    rationale: format!(
                        "{} documents have had no modifications for over {} days.",
                        ids.len(),
                        self.options.stale_days_threshold
                    ),
                    confidence: 0.70,
                    affected_file_ids: ids,
                    suggested_tag_id: None,
                    suggested_tag_name: Some("archive".to_string()),
                    suggested_collection_id: None,
                    suggested_collection_name: None,
                });
            }
        }

        suggestions
    }

    /// Converts detected duplicate groups into user-facing RawSuggestions of type "duplicate_files".
    pub fn duplicate_groups_to_suggestions(
        &self,
        groups: &[RawDuplicateGroup],
        files: &[IndexedFile],
    ) -> Vec<RawSuggestion> {
        let file_map: HashMap<i64, &IndexedFile> = files.iter().map(|f| (f.id, f)).collect();
        let mut suggestions = Vec::new();

        for group in groups {
            if group.file_ids.is_empty() {
                continue;
            }
            let first_file = file_map.get(&group.file_ids[0]);
            let sample_name = first_file
                .map(|f| f.file_name.as_str())
                .unwrap_or("document");

            let (title, rationale) = match group.match_type.as_str() {
                "file_identity" => (
                    format!("Identical file identity detected: '{sample_name}'"),
                    format!(
                        "Detected {} files sharing the same file system identity (hard link or identical NTFS file reference).",
                        group.file_ids.len()
                    ),
                ),
                "exact_hash" => (
                    format!("Duplicate file contents detected: '{sample_name}'"),
                    format!(
                        "Detected {} files with identical SHA-256 content hashes and identical byte sizes.",
                        group.file_ids.len()
                    ),
                ),
                "name_and_size" => (
                    format!("Likely duplicates detected: '{sample_name}'"),
                    format!(
                        "Detected {} files with identical names and identical byte sizes across directories.",
                        group.file_ids.len()
                    ),
                ),
                _ => (
                    format!("Duplicate files detected: '{sample_name}'"),
                    format!("Detected {} potential duplicate files.", group.file_ids.len()),
                ),
            };

            suggestions.push(RawSuggestion {
                suggestion_type: "duplicate_files".to_string(),
                title,
                rationale,
                confidence: group.confidence,
                affected_file_ids: group.file_ids.clone(),
                suggested_tag_id: None,
                suggested_tag_name: Some("duplicate".to_string()),
                suggested_collection_id: None,
                suggested_collection_name: None,
            });
        }

        suggestions
    }

    /// Runs all analyzers and returns the combined set of suggestions and duplicate groups.
    pub fn analyze_all(
        &self,
        files: &[IndexedFile],
        collections: &[Collection],
        tags: &[Tag],
        file_collection_map: &HashMap<i64, Vec<i64>>,
        file_tag_map: &HashMap<i64, Vec<i64>>,
        now: DateTime<Utc>,
    ) -> (Vec<RawSuggestion>, Vec<RawDuplicateGroup>) {
        let duplicates = self.find_duplicates(files);
        let dup_suggestions = self.duplicate_groups_to_suggestions(&duplicates, files);
        let versions = self.find_likely_versions(files);
        let col_suggestions =
            self.find_suggested_collections(files, collections, file_collection_map);
        let tag_suggestions = self.find_suggested_tags(files, tags, file_tag_map);
        let stale_suggestions = self.find_stale_files(files, now);

        let mut all_suggestions = Vec::new();
        all_suggestions.extend(dup_suggestions);
        all_suggestions.extend(versions);
        all_suggestions.extend(col_suggestions);
        all_suggestions.extend(tag_suggestions);
        all_suggestions.extend(stale_suggestions);

        (all_suggestions, duplicates)
    }

    /// Evaluates the library using the repository and transactionally persists pending suggestions.
    pub fn run_and_persist(&self, db: &crate::db::Database) -> rusqlite::Result<(usize, usize)> {
        let files = db.list_files(None, None, None, None, None)?;
        let collections = db.list_collections()?;
        let tags = db.list_tags()?;

        let col_links = db.list_file_collection_links(None, None)?;
        let mut file_collection_map: HashMap<i64, Vec<i64>> = HashMap::new();
        for link in col_links {
            file_collection_map
                .entry(link.file_id)
                .or_default()
                .push(link.collection_id);
        }

        let tag_links = db.list_file_tag_links(None, None)?;
        let mut file_tag_map: HashMap<i64, Vec<i64>> = HashMap::new();
        for link in tag_links {
            file_tag_map
                .entry(link.file_id)
                .or_default()
                .push(link.tag_id);
        }

        let (suggestions, duplicate_groups) = self.analyze_all(
            &files,
            &collections,
            &tags,
            &file_collection_map,
            &file_tag_map,
            Utc::now(),
        );

        // Clear existing pending suggestions and duplicate groups before storing newly generated pending sets
        db.clear_pending_organization_suggestions()?;
        db.clear_pending_duplicate_groups()?;

        // Persist suggestions
        let mut sugg_count = 0;
        for s in &suggestions {
            db.insert_organization_suggestion(
                &s.suggestion_type,
                &s.title,
                &s.rationale,
                s.confidence,
                &s.affected_file_ids,
                s.suggested_tag_id,
                s.suggested_tag_name.as_deref(),
                s.suggested_collection_id,
                s.suggested_collection_name.as_deref(),
            )?;
            sugg_count += 1;
        }

        // Persist duplicate groups
        let mut dup_count = 0;
        for g in &duplicate_groups {
            db.insert_duplicate_group(&g.match_type, g.confidence, &g.file_ids)?;
            dup_count += 1;
        }

        Ok((sugg_count, dup_count))
    }
}

// ── Path & Stem Heuristic Helpers ─────────────────────────────────────────────

fn get_parent_directory(normalized_path: &str) -> String {
    if let Some(idx) = normalized_path.rfind('\\') {
        normalized_path[..idx].to_string()
    } else {
        String::new()
    }
}

fn get_last_path_component(path: &str) -> String {
    path.rsplit('\\')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn get_filename_stem(filename: &str) -> String {
    if let Some(idx) = filename.rfind('.') {
        filename[..idx].to_string()
    } else {
        filename.to_string()
    }
}

/// Normalizes a stem to extract the base version stem (e.g. "report_v2" -> "report", "doc-final" -> "doc").
fn extract_version_base_stem(stem: &str) -> String {
    let lower = stem.to_lowercase();
    let delimiters = ['_', '-', ' ', '.'];

    let tokens: Vec<&str> = lower
        .split(|c| delimiters.contains(&c))
        .filter(|t| !t.is_empty())
        .collect();

    if tokens.is_empty() {
        return lower;
    }

    // Strip trailing version markers: v1, v2, ver1, final, draft, copy, rev1, edit
    let mut base_tokens = Vec::new();
    for token in tokens {
        if is_version_marker(token) {
            continue;
        }
        base_tokens.push(token);
    }

    if base_tokens.is_empty() {
        lower
    } else {
        base_tokens.join("_")
    }
}

fn is_version_marker(token: &str) -> bool {
    if token == "final"
        || token == "draft"
        || token == "copy"
        || token == "rev"
        || token == "new"
        || token == "old"
        || token == "latest"
    {
        return true;
    }

    // v1, v02, ver1, rev2, etc.
    let stripped = token
        .trim_start_matches('v')
        .trim_start_matches("ver")
        .trim_start_matches("rev");

    if !stripped.is_empty() && stripped.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    false
}

fn is_generic_directory(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "documents" | "desktop" | "downloads" | "files" | "data" | "docs" | "folder" | "new folder"
    )
}

fn to_title_case(s: &str) -> String {
    let mut res = String::new();
    let mut prev_char: Option<char> = None;
    let mut capitalize = true;

    for c in s.chars() {
        if c == '_' || c == '-' || c == ' ' {
            if !res.is_empty() && !res.ends_with(' ') {
                res.push(' ');
            }
            capitalize = true;
            prev_char = Some(' ');
        } else if c.is_uppercase() {
            if let Some(p) = prev_char {
                if (p.is_lowercase() || p.is_ascii_digit()) && !res.ends_with(' ') {
                    res.push(' ');
                }
            }
            res.extend(c.to_uppercase());
            capitalize = false;
            prev_char = Some(c);
        } else {
            if capitalize {
                res.extend(c.to_uppercase());
                capitalize = false;
            } else {
                res.push(c);
            }
            prev_char = Some(c);
        }
    }

    res.trim().to_string()
}

fn extract_year(text: &str) -> Option<String> {
    // Looks for 4-digit numbers 1990..=2050 bounded by non-digits
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    for i in 0..len {
        if i + 4 <= len {
            let slice: String = chars[i..i + 4].iter().collect();
            if let Ok(year) = slice.parse::<u32>() {
                if (1990..=2050).contains(&year) {
                    let prev_ok = i == 0 || !chars[i - 1].is_ascii_digit();
                    let next_ok = i + 4 == len || !chars[i + 4].is_ascii_digit();
                    if prev_ok && next_ok {
                        return Some(slice);
                    }
                }
            }
        }
    }
    None
}

fn tokenize_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .map(|s| s.to_lowercase())
        .collect()
}

fn is_common_document_keyword(word: &str) -> bool {
    matches!(
        word,
        "invoice"
            | "receipt"
            | "contract"
            | "agreement"
            | "statement"
            | "tax"
            | "taxes"
            | "report"
            | "resume"
            | "manual"
            | "draft"
            | "proposal"
            | "summary"
            | "policy"
            | "specification"
            | "budget"
            | "audit"
            | "notes"
    )
}

fn get_extension_category(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "pdf" => Some("pdf"),
        "docx" | "doc" | "odt" | "rtf" => Some("document"),
        "xlsx" | "xls" | "csv" | "ods" => Some("spreadsheet"),
        "pptx" | "ppt" | "odp" => Some("presentation"),
        "txt" | "md" => Some("text"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{Collection, IndexedFile, NewIndexedFile, Tag};
    use chrono::{Duration, Utc};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_test_file(
        id: i64,
        path: &str,
        name: &str,
        ext: &str,
        size: i64,
        hash: Option<&str>,
        file_id: Option<&str>,
        status: &str,
        mod_at: Option<&str>,
        last_seen: &str,
    ) -> IndexedFile {
        IndexedFile {
            id,
            scan_root_id: 1,
            absolute_path: path.to_string(),
            normalized_path: path.replace('/', "\\"),
            file_name: name.to_string(),
            extension: ext.to_string(),
            size_bytes: size,
            modified_at: mod_at.map(String::from),
            created_at_if_available: None,
            discovered_at: last_seen.to_string(),
            last_seen_at: last_seen.to_string(),
            status: status.to_string(),
            content_hash_nullable: hash.map(String::from),
            file_id_nullable: file_id.map(String::from),
        }
    }

    fn temp_db() -> (Database, PathBuf) {
        let p = std::env::temp_dir().join(format!(
            "libone-analyzer-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (Database::new(&p).unwrap(), p)
    }

    #[test]
    fn test_exact_duplicates_by_file_identity() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                1,
                r"C:\FolderA\contract.pdf",
                "contract.pdf",
                "pdf",
                5000,
                None,
                Some("VOL_1:ID_12345"),
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                2,
                r"C:\FolderB\link_contract.pdf",
                "link_contract.pdf",
                "pdf",
                5000,
                None,
                Some("VOL_1:ID_12345"),
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                3,
                r"C:\FolderC\other.pdf",
                "other.pdf",
                "pdf",
                5000,
                None,
                Some("VOL_1:ID_99999"),
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let dups = engine.find_duplicates(&files);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].match_type, "file_identity");
        assert_eq!(dups[0].confidence, 1.0);
        assert_eq!(dups[0].file_ids.len(), 2);
        assert!(dups[0].file_ids.contains(&1) && dups[0].file_ids.contains(&2));

        let suggs = engine.duplicate_groups_to_suggestions(&dups, &files);
        assert_eq!(suggs.len(), 1);
        assert_eq!(suggs[0].suggestion_type, "duplicate_files");
        assert_eq!(suggs[0].confidence, 1.0);
        assert!(suggs[0].rationale.contains("file system identity"));
    }

    #[test]
    fn test_exact_duplicates_by_content_hash() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                10,
                r"C:\A\presentation.pptx",
                "presentation.pptx",
                "pptx",
                8192,
                Some("sha256:hash_abc_123"),
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                11,
                r"C:\B\presentation_backup.pptx",
                "presentation_backup.pptx",
                "pptx",
                8192,
                Some("sha256:hash_abc_123"),
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let dups = engine.find_duplicates(&files);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].match_type, "exact_hash");
        assert_eq!(dups[0].confidence, 1.0);
        assert_eq!(dups[0].file_ids, vec![10, 11]);

        let suggs = engine.duplicate_groups_to_suggestions(&dups, &files);
        assert_eq!(suggs.len(), 1);
        assert_eq!(suggs[0].suggestion_type, "duplicate_files");
        assert_eq!(suggs[0].confidence, 1.0);
        assert!(suggs[0].rationale.contains("identical SHA-256"));
    }

    #[test]
    fn test_heuristic_duplicates_by_name_and_size() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                20,
                r"C:\A\manual.pdf",
                "manual.pdf",
                "pdf",
                123456,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                21,
                r"C:\B\manual.pdf",
                "manual.pdf",
                "pdf",
                123456,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let dups = engine.find_duplicates(&files);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].match_type, "name_and_size");
        assert_eq!(dups[0].confidence, 0.85);

        let suggs = engine.duplicate_groups_to_suggestions(&dups, &files);
        assert_eq!(suggs.len(), 1);
        assert_eq!(suggs[0].confidence, 0.85);
        assert!(suggs[0]
            .rationale
            .contains("identical names and identical byte sizes"));
    }

    #[test]
    fn test_duplicates_ignores_missing_and_zero_byte_files() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                30,
                r"C:\A\empty.txt",
                "empty.txt",
                "txt",
                0,
                Some("sha256:empty_hash"),
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                31,
                r"C:\B\empty.txt",
                "empty.txt",
                "txt",
                0,
                Some("sha256:empty_hash"),
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                32,
                r"C:\C\doc.pdf",
                "doc.pdf",
                "pdf",
                500,
                Some("sha256:same"),
                None,
                "missing",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                33,
                r"C:\D\doc.pdf",
                "doc.pdf",
                "pdf",
                500,
                Some("sha256:same"),
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let dups = engine.find_duplicates(&files);
        assert_eq!(
            dups.len(),
            0,
            "Missing files and 0-byte files should not form duplicate groups"
        );
    }

    #[test]
    fn test_likely_versions_detection() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                1,
                r"C:\Reports\Quarterly_Report.pdf",
                "Quarterly_Report.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                2,
                r"C:\Reports\Quarterly_Report_v2.pdf",
                "Quarterly_Report_v2.pdf",
                "pdf",
                110,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                3,
                r"C:\Reports\Quarterly_Report_final.pdf",
                "Quarterly_Report_final.pdf",
                "pdf",
                120,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                4,
                r"C:\Reports\Unrelated_Document.pdf",
                "Unrelated_Document.pdf",
                "pdf",
                50,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let versions = engine.find_likely_versions(&files);
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].suggestion_type, "likely_version");
        assert_eq!(versions[0].confidence, 0.85);
        assert_eq!(versions[0].affected_file_ids, vec![1, 2, 3]);
        assert!(versions[0].title.contains("quarterly_report"));
        assert!(versions[0].rationale.contains("3 related versions"));
    }

    #[test]
    fn test_suggested_collections_from_folder_clusters() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                1,
                r"C:\Work\ProjectAlpha\doc1.pdf",
                "doc1.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                2,
                r"C:\Work\ProjectAlpha\doc2.pdf",
                "doc2.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                3,
                r"C:\Work\ProjectAlpha\doc3.pdf",
                "doc3.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            // Generic directory should be ignored
            make_test_file(
                4,
                r"C:\Documents\a.pdf",
                "a.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                5,
                r"C:\Documents\b.pdf",
                "b.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                6,
                r"C:\Documents\c.pdf",
                "c.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let col_map = HashMap::new();
        let suggestions = engine.find_suggested_collections(&files, &[], &col_map);

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].suggestion_type, "suggest_collection");
        assert_eq!(
            suggestions[0].suggested_collection_name,
            Some("Project Alpha".to_string())
        );
        assert_eq!(suggestions[0].confidence, 0.80);
        assert_eq!(suggestions[0].affected_file_ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_suggested_collections_maps_to_existing_collection() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                1,
                r"C:\Work\Finances\q1.xlsx",
                "q1.xlsx",
                "xlsx",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                2,
                r"C:\Work\Finances\q2.xlsx",
                "q2.xlsx",
                "xlsx",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                3,
                r"C:\Work\Finances\q3.xlsx",
                "q3.xlsx",
                "xlsx",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                4,
                r"C:\Work\Finances\q4.xlsx",
                "q4.xlsx",
                "xlsx",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let existing_cols = vec![Collection {
            id: 42,
            name: "finances".to_string(),
            description_nullable: None,
            color_nullable: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }];

        // File 1 is already linked to collection 42
        let mut col_map = HashMap::new();
        col_map.insert(1, vec![42]);

        let suggestions = engine.find_suggested_collections(&files, &existing_cols, &col_map);

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].suggested_collection_id, Some(42));
        assert_eq!(
            suggestions[0].suggested_collection_name,
            Some("finances".to_string())
        );
        // Files 2, 3, 4 are unlinked and recommended
        assert_eq!(suggestions[0].affected_file_ids, vec![2, 3, 4]);
    }

    #[test]
    fn test_suggested_tags_year_and_keywords_and_extension() {
        let engine = SuggestionEngine::default();
        let files = vec![
            make_test_file(
                1,
                r"C:\Docs\Invoice_2024_January.pdf",
                "Invoice_2024_January.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
            make_test_file(
                2,
                r"C:\Docs\Invoice_2024_February.pdf",
                "Invoice_2024_February.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                None,
                "2026-11-20T00:00:00Z",
            ),
        ];

        let existing_tags = vec![Tag {
            id: 7,
            name: "invoice".to_string(),
            color_nullable: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }];

        let tag_map = HashMap::new();
        let suggestions = engine.find_suggested_tags(&files, &existing_tags, &tag_map);

        // Should suggest:
        // - "invoice" (matches existing tag 7, confidence 0.90)
        // - "2024" (new tag, confidence 0.75)
        // - "pdf" (extension tag, confidence 0.75)
        let invoice_sugg = suggestions
            .iter()
            .find(|s| s.suggested_tag_name.as_deref() == Some("invoice"));
        assert!(invoice_sugg.is_some());
        let inv = invoice_sugg.unwrap();
        assert_eq!(inv.suggested_tag_id, Some(7));
        assert_eq!(inv.confidence, 0.90);
        assert_eq!(inv.affected_file_ids, vec![1, 2]);

        let year_sugg = suggestions
            .iter()
            .find(|s| s.suggested_tag_name.as_deref() == Some("2024"));
        assert!(year_sugg.is_some());
        let yr = year_sugg.unwrap();
        assert_eq!(yr.suggested_tag_id, None);
        assert_eq!(yr.confidence, 0.75);
        assert_eq!(yr.affected_file_ids, vec![1, 2]);
    }

    #[test]
    fn test_stale_files_detection() {
        let engine = SuggestionEngine::default(); // 180 days threshold
        let now = Utc::now();
        let days_200_ago = (now - Duration::days(200)).to_rfc3339();
        let days_10_ago = (now - Duration::days(10)).to_rfc3339();

        let files = vec![
            make_test_file(
                1,
                r"C:\Files\missing_doc.pdf",
                "missing_doc.pdf",
                "pdf",
                100,
                None,
                None,
                "missing",
                None,
                &days_200_ago,
            ),
            make_test_file(
                2,
                r"C:\Files\old_doc.pdf",
                "old_doc.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                Some(&days_200_ago),
                &days_10_ago,
            ),
            make_test_file(
                3,
                r"C:\Files\recent_doc.pdf",
                "recent_doc.pdf",
                "pdf",
                100,
                None,
                None,
                "indexed",
                Some(&days_10_ago),
                &days_10_ago,
            ),
        ];

        let suggestions = engine.find_stale_files(&files, now);
        assert_eq!(suggestions.len(), 2);

        let missing_sugg = suggestions
            .iter()
            .find(|s| s.title.contains("stale missing index records"));
        assert!(missing_sugg.is_some());
        assert_eq!(missing_sugg.unwrap().affected_file_ids, vec![1]);
        assert_eq!(missing_sugg.unwrap().confidence, 0.95);

        let archive_sugg = suggestions
            .iter()
            .find(|s| s.title.contains("Archive candidate"));
        assert!(archive_sugg.is_some());
        assert_eq!(archive_sugg.unwrap().affected_file_ids, vec![2]);
        assert_eq!(archive_sugg.unwrap().confidence, 0.70);
    }

    #[test]
    fn test_run_and_persist_with_temporary_database() {
        let (db, path) = temp_db();

        let root = db.add_scan_root(r"C:\TestLibrary", "TestLibrary").unwrap();

        // Insert duplicate pair
        let f1 = db
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: r"C:\TestLibrary\FolderA\contract.pdf".into(),
                normalized_path: r"C:\TestLibrary\FolderA\contract.pdf".into(),
                file_name: "contract.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 4096,
                modified_at: Some("2026-11-01T00:00:00Z".into()),
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("hash_exact_123".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let f2 = db
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: r"C:\TestLibrary\FolderB\contract_copy.pdf".into(),
                normalized_path: r"C:\TestLibrary\FolderB\contract_copy.pdf".into(),
                file_name: "contract_copy.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 4096,
                modified_at: Some("2026-11-01T00:00:00Z".into()),
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("hash_exact_123".into()),
                file_id_nullable: None,
            })
            .unwrap();

        // Insert folder cluster for suggested collection (3 files in ProjectX)
        let f3 = db
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: r"C:\TestLibrary\ProjectX\spec1.docx".into(),
                normalized_path: r"C:\TestLibrary\ProjectX\spec1.docx".into(),
                file_name: "spec1.docx".into(),
                extension: "docx".into(),
                size_bytes: 1000,
                modified_at: Some("2026-11-01T00:00:00Z".into()),
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let f4 = db
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: r"C:\TestLibrary\ProjectX\spec2.docx".into(),
                normalized_path: r"C:\TestLibrary\ProjectX\spec2.docx".into(),
                file_name: "spec2.docx".into(),
                extension: "docx".into(),
                size_bytes: 1100,
                modified_at: Some("2026-11-01T00:00:00Z".into()),
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let f5 = db
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: r"C:\TestLibrary\ProjectX\spec3.docx".into(),
                normalized_path: r"C:\TestLibrary\ProjectX\spec3.docx".into(),
                file_name: "spec3.docx".into(),
                extension: "docx".into(),
                size_bytes: 1200,
                modified_at: Some("2026-11-01T00:00:00Z".into()),
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // Execute engine analysis and persistence
        let (sugg_count, dup_count) = db.generate_and_store_suggestions(None).unwrap();
        assert!(
            sugg_count > 0,
            "Expected suggestions to be generated and stored"
        );
        assert_eq!(dup_count, 1, "Expected 1 duplicate group");

        // Verify duplicate groups in SQLite
        let dups = db.list_duplicate_groups(None).unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].status, "pending");
        assert_eq!(dups[0].confidence, 1.0);
        assert_eq!(dups[0].match_type, "exact_hash");
        assert!(dups[0].file_ids.contains(&f1.id) && dups[0].file_ids.contains(&f2.id));

        // Verify organization suggestions in SQLite
        let suggs = db.list_organization_suggestions(None, None).unwrap();
        assert_eq!(suggs.len(), sugg_count);

        let allowed_types = [
            "duplicate_files",
            "likely_version",
            "suggest_collection",
            "suggest_tag",
            "stale_file",
        ];

        for s in &suggs {
            assert!(
                allowed_types.contains(&s.suggestion_type.as_str()),
                "Unknown suggestion_type: {}",
                s.suggestion_type
            );
            assert_eq!(s.status, "pending");
            assert!(s.confidence >= 0.0 && s.confidence <= 1.0);
            assert!(!s.title.trim().is_empty());
            assert!(!s.rationale.trim().is_empty());
            assert!(!s.affected_file_ids.is_empty());
            assert!(s.reviewed_at.is_none());
        }

        // Verify folder cluster suggestion exists for ProjectX
        let col_sugg = suggs
            .iter()
            .find(|s| s.suggestion_type == "suggest_collection")
            .expect("Expected suggest_collection");
        assert_eq!(
            col_sugg.suggested_collection_name.as_deref(),
            Some("Project X")
        );
        assert!(col_sugg.affected_file_ids.contains(&f3.id));
        assert!(col_sugg.affected_file_ids.contains(&f4.id));
        assert!(col_sugg.affected_file_ids.contains(&f5.id));

        // Approve one suggestion
        db.update_suggestion_status(col_sugg.id, "approved")
            .unwrap();
        let approved = db
            .get_organization_suggestion(col_sugg.id)
            .unwrap()
            .unwrap();
        assert_eq!(approved.status, "approved");
        assert!(approved.reviewed_at.is_some());

        // Re-run suggestion engine: pending suggestions refreshed, approved suggestion preserved
        let (new_sugg_count, new_dup_count) = db.generate_and_store_suggestions(None).unwrap();
        assert!(new_sugg_count > 0);
        assert_eq!(new_dup_count, 1);

        let approved_after = db
            .get_organization_suggestion(col_sugg.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            approved_after.status, "approved",
            "Approved suggestion must remain preserved across suggestion runs"
        );

        drop(db);
        let _ = std::fs::remove_dir_all(path);
    }
}
