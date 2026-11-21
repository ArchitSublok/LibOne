import React, { useState, useEffect, useMemo } from "react";
import {
  AlertCircle,
  AlertTriangle,
  ArrowLeft,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock,
  Copy,
  EyeOff,
  FileText,
  Filter,
  FolderPlus,
  Info,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Tag as TagIcon,
  X,
} from "lucide-react";
import { DuplicateGroup, IndexedFile, LocalAiStatus, OrganizationSuggestion } from "../types";
import {
  approveSuggestion,
  bulkApproveSuggestions,
  bulkRejectSuggestions,
  dismissSuggestion,
  generateSuggestions,
  getIndexedFileDetail,
  getLocalAiStatus,
  listDuplicateGroups,
  listOrganizationSuggestions,
  rejectSuggestion,
} from "../services/api";
import { DuplicateGroupDetailView } from "./DuplicateGroupDetailView";
import { VersionGroupDisplay, StaleFileExplanation } from "./RelationshipViews";

interface Props {
  onBackToLibrary: () => void;
  onSuggestionsChanged?: () => void;
}

type TypeFilter =
  | "all"
  | "duplicate_files"
  | "likely_version"
  | "suggest_collection"
  | "suggest_tag"
  | "stale_file";

type ConfidenceFilter = "all" | "high" | "medium";
type StatusFilter = "pending" | "approved" | "rejected" | "dismissed" | "all";

export const SuggestionsView: React.FC<Props> = ({
  onBackToLibrary,
  onSuggestionsChanged,
}) => {
  // ── State ──────────────────────────────────────────────────────────────────
  const [suggestions, setSuggestions] = useState<OrganizationSuggestion[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionSuccess, setActionSuccess] = useState<string | null>(null);
  const [processingId, setProcessingId] = useState<number | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState(false);

  // Filters
  const [typeFilter, setTypeFilter] = useState<TypeFilter>("all");
  const [confidenceFilter, setConfidenceFilter] = useState<ConfidenceFilter>("all");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("pending");

  // Selection for bulk actions
  const [selectedIds, setSelectedIds] = useState<number[]>([]);

  // Expanded affected files drawer and file cache
  const [expandedSuggestionIds, setExpandedSuggestionIds] = useState<Set<number>>(
    new Set()
  );
  const [fileCache, setFileCache] = useState<Record<number, IndexedFile>>({});
  const [filesLoading, setFilesLoading] = useState<Record<number, boolean>>({});

  // Duplicate group detail view state
  const [selectedDuplicateGroupId, setSelectedDuplicateGroupId] = useState<number | null>(null);
  const [duplicateGroups, setDuplicateGroups] = useState<DuplicateGroup[]>([]);

  // Optional local intelligence provider status
  const [localAiStatus, setLocalAiStatus] = useState<LocalAiStatus | null>(null);

  // ── Load suggestions ───────────────────────────────────────────────────────
  const loadSuggestions = async () => {
    setLoading(true);
    setError(null);
    try {
      const [data, dGroups, aiStat] = await Promise.all([
        listOrganizationSuggestions(
          statusFilter === "all" ? null : statusFilter,
          typeFilter === "all" ? null : typeFilter
        ),
        listDuplicateGroups(statusFilter === "all" ? null : statusFilter),
        getLocalAiStatus().catch(() => ({
          is_available: false,
          provider_name: "DisabledLocalProvider",
          status_message: "Local AI is not configured",
          model_name: null,
          allows_network: false,
        })),
      ]);
      setSuggestions(data);
      setDuplicateGroups(dGroups);
      setLocalAiStatus(aiStat);
      if (onSuggestionsChanged) {
        onSuggestionsChanged();
      }
    } catch (e: any) {
      setError(e?.message || "Failed to load suggestions.");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSuggestions();
  }, [statusFilter, typeFilter]);

  // ── Pending count ─────────────────────────────────────────────────────────
  const pendingCount = useMemo(() => {
    return suggestions.filter((s) => s.status.toLowerCase() === "pending").length;
  }, [suggestions]);

  // ── Filtered items by confidence ──────────────────────────────────────────
  const filteredSuggestions = useMemo(() => {
    return suggestions.filter((s) => {
      if (confidenceFilter === "high" && s.confidence < 0.85) {
        return false;
      }
      if (confidenceFilter === "medium" && s.confidence < 0.75) {
        return false;
      }
      return true;
    });
  }, [suggestions, confidenceFilter]);

  // ── Separate sections ─────────────────────────────────────────────────────
  const duplicateSuggestions = useMemo(
    () => filteredSuggestions.filter((s) => s.suggestion_type === "duplicate_files"),
    [filteredSuggestions]
  );

  const versionSuggestions = useMemo(
    () => filteredSuggestions.filter((s) => s.suggestion_type === "likely_version"),
    [filteredSuggestions]
  );

  const collectionSuggestions = useMemo(
    () => filteredSuggestions.filter((s) => s.suggestion_type === "suggest_collection"),
    [filteredSuggestions]
  );

  const tagSuggestions = useMemo(
    () => filteredSuggestions.filter((s) => s.suggestion_type === "suggest_tag"),
    [filteredSuggestions]
  );

  const staleSuggestions = useMemo(
    () => filteredSuggestions.filter((s) => s.suggestion_type === "stale_file"),
    [filteredSuggestions]
  );

  // ── Load affected file details on expand ──────────────────────────────────
  const toggleExpand = async (suggestionId: number, fileIds: number[]) => {
    setExpandedSuggestionIds((prev) => {
      const next = new Set(prev);
      if (next.has(suggestionId)) {
        next.delete(suggestionId);
      } else {
        next.add(suggestionId);
      }
      return next;
    });

    const missingIds = fileIds.filter((id) => !fileCache[id] && !filesLoading[id]);
    if (missingIds.length === 0) return;

    setFilesLoading((prev) => {
      const next = { ...prev };
      for (const id of missingIds) next[id] = true;
      return next;
    });

    for (const fid of missingIds) {
      try {
        const detail = await getIndexedFileDetail(fid);
        setFileCache((prev) => ({ ...prev, [fid]: detail.file }));
      } catch {
        // If file not found or error, continue gracefully
      } finally {
        setFilesLoading((prev) => {
          const next = { ...prev };
          delete next[fid];
          return next;
        });
      }
    }
  };

  // ── Action handlers ───────────────────────────────────────────────────────
  const handleApprove = async (suggestion: OrganizationSuggestion) => {
    setProcessingId(suggestion.id);
    setError(null);
    try {
      await approveSuggestion(suggestion.id);
      setActionSuccess(
        `Approved: "${suggestion.title}". LibOne metadata updated. Original files remain untouched.`
      );
      await loadSuggestions();
    } catch (e: any) {
      setError(`Approval failed: ${e?.message || e}`);
    } finally {
      setProcessingId(null);
    }
  };

  const handleReject = async (suggestion: OrganizationSuggestion) => {
    setProcessingId(suggestion.id);
    setError(null);
    try {
      await rejectSuggestion(suggestion.id);
      setActionSuccess(`Rejected: "${suggestion.title}".`);
      await loadSuggestions();
    } catch (e: any) {
      setError(`Reject failed: ${e?.message || e}`);
    } finally {
      setProcessingId(null);
    }
  };

  const handleDismiss = async (suggestion: OrganizationSuggestion) => {
    setProcessingId(suggestion.id);
    setError(null);
    try {
      await dismissSuggestion(suggestion.id);
      setActionSuccess(`Dismissed: "${suggestion.title}".`);
      await loadSuggestions();
    } catch (e: any) {
      setError(`Dismiss failed: ${e?.message || e}`);
    } finally {
      setProcessingId(null);
    }
  };

  // ── Bulk Actions ──────────────────────────────────────────────────────────
  const toggleSelect = (id: number) => {
    setSelectedIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]
    );
  };

  const handleSelectAllVisible = () => {
    if (selectedIds.length === filteredSuggestions.length) {
      setSelectedIds([]);
    } else {
      setSelectedIds(filteredSuggestions.map((s) => s.id));
    }
  };

  const handleBulkApprove = async () => {
    if (selectedIds.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      const res = await bulkApproveSuggestions(selectedIds);
      setActionSuccess(
        `Bulk approved ${res.approved_ids.length} suggestions. LibOne metadata updated. Original files remain untouched.`
      );
      if (res.errors.length > 0) {
        setError(`Some suggestions could not be approved: ${res.errors.join("; ")}`);
      }
      setSelectedIds([]);
      await loadSuggestions();
    } catch (e: any) {
      setError(`Bulk approve failed: ${e?.message || e}`);
    } finally {
      setLoading(false);
    }
  };

  const handleBulkReject = async () => {
    if (selectedIds.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      const res = await bulkRejectSuggestions(selectedIds);
      setActionSuccess(`Bulk rejected ${res.rejected_ids.length} suggestions.`);
      if (res.errors.length > 0) {
        setError(`Some suggestions could not be rejected: ${res.errors.join("; ")}`);
      }
      setSelectedIds([]);
      await loadSuggestions();
    } catch (e: any) {
      setError(`Bulk reject failed: ${e?.message || e}`);
    } finally {
      setLoading(false);
    }
  };

  // ── Trigger Engine Analysis ───────────────────────────────────────────────
  const handleAnalyzeLibrary = async () => {
    setIsAnalyzing(true);
    setError(null);
    try {
      const [suggCount, dupCount] = await generateSuggestions();
      setActionSuccess(
        `Library analysis complete: generated ${suggCount} organization suggestions and ${dupCount} duplicate groups.`
      );
      await loadSuggestions();
    } catch (e: any) {
      setError(`Analysis failed: ${e?.message || e}`);
    } finally {
      setIsAnalyzing(false);
    }
  };

  // ── Format helpers ────────────────────────────────────────────────────────
  const formatConfidence = (conf: number): string => {
    return `${Math.round(conf * 100)}%`;
  };

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  // ── Render single suggestion card ─────────────────────────────────────────
  const renderCard = (suggestion: OrganizationSuggestion) => {
    const isExpanded = expandedSuggestionIds.has(suggestion.id);
    const isProcessing = processingId === suggestion.id;
    const isSelected = selectedIds.includes(suggestion.id);

    const matchingGroup =
      suggestion.suggestion_type === "duplicate_files"
        ? duplicateGroups.find((g) => {
            if (g.file_ids.length !== suggestion.affected_file_ids.length) return false;
            return g.file_ids.every((fid) => suggestion.affected_file_ids.includes(fid));
          }) || (duplicateGroups.length > 0 ? duplicateGroups[0] : null)
        : null;

    const confidenceClass =
      suggestion.confidence >= 0.85
        ? "confidence-badge--high"
        : suggestion.confidence >= 0.75
        ? "confidence-badge--medium"
        : "confidence-badge--low";

    const statusBadgeClass =
      suggestion.status === "approved"
        ? "badge-success"
        : suggestion.status === "rejected"
        ? "badge-error"
        : suggestion.status === "dismissed"
        ? "badge-secondary"
        : "badge-warning";

    return (
      <div
        key={suggestion.id}
        className={`suggestion-card ${
          suggestion.status !== "pending" ? "suggestion-card--reviewed" : ""
        } ${isSelected ? "suggestion-card--selected" : ""}`}
      >
        <div className="suggestion-card__header">
          <div className="suggestion-card__meta">
            <input
              type="checkbox"
              className="suggestion-checkbox"
              checked={isSelected}
              onChange={() => toggleSelect(suggestion.id)}
              title="Select for bulk action"
              aria-label={`Select suggestion ${suggestion.title}`}
            />
            <span className={`badge ${statusBadgeClass}`}>
              {suggestion.status.toUpperCase()}
            </span>
            <span
              className={`confidence-badge ${confidenceClass}`}
              title={`Confidence: ${formatConfidence(suggestion.confidence)}`}
            >
              {formatConfidence(suggestion.confidence)} confidence
            </span>
            {suggestion.suggested_collection_name && (
              <span className="badge badge-collection-tag">
                <FolderPlus size={11} />
                Collection: {suggestion.suggested_collection_name}
              </span>
            )}
            {suggestion.suggested_tag_name && (
              <span className="badge badge-collection-tag">
                <TagIcon size={11} />
                Tag: {suggestion.suggested_tag_name}
              </span>
            )}
          </div>

          {/* Action buttons (approval requires explicit user click) */}
          <div className="suggestion-card__actions">
            {matchingGroup && (
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                onClick={() => setSelectedDuplicateGroupId(matchingGroup.id)}
                title="Open side-by-side file comparison and mark preferred version"
              >
                <Copy size={13} />
                <span>Compare & Review</span>
              </button>
            )}
            {suggestion.status === "pending" ? (
              <>
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  onClick={() => handleApprove(suggestion)}
                  disabled={isProcessing}
                  title="Approve: applies metadata without altering original files"
                >
                  <Check size={13} />
                  <span>{isProcessing ? "Approving…" : "Approve"}</span>
                </button>
                <button
                  type="button"
                  className="btn btn-secondary btn-sm"
                  onClick={() => handleReject(suggestion)}
                  disabled={isProcessing}
                  title="Reject this suggestion"
                >
                  <X size={13} />
                  <span>Reject</span>
                </button>
                <button
                  type="button"
                  className="btn btn-secondary btn-sm"
                  onClick={() => handleDismiss(suggestion)}
                  disabled={isProcessing}
                  title="Dismiss this suggestion"
                >
                  <EyeOff size={13} />
                  <span>Dismiss</span>
                </button>
              </>
            ) : (
              <span className="reviewed-timestamp">
                Reviewed {suggestion.reviewed_at ? new Date(suggestion.reviewed_at).toLocaleDateString() : ""}
              </span>
            )}
          </div>
        </div>

        <div className="suggestion-card__body">
          <h4 className="suggestion-card__title">{suggestion.title}</h4>
          <p className="suggestion-card__rationale">{suggestion.rationale}</p>
        </div>

        {/* Affected files section */}
        <div className="suggestion-card__footer">
          <button
            type="button"
            className="btn-toggle-files"
            onClick={() => toggleExpand(suggestion.id, suggestion.affected_file_ids)}
          >
            {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            <span>
              {suggestion.affected_file_ids.length}{" "}
              {suggestion.affected_file_ids.length === 1 ? "affected file" : "affected files"}
            </span>
          </button>

          {isExpanded && (
            <div className="affected-files-expanded-wrapper">
              {suggestion.suggestion_type === "likely_version" ? (
                <VersionGroupDisplay suggestion={suggestion} />
              ) : suggestion.suggestion_type === "stale_file" ? (
                <StaleFileExplanation suggestion={suggestion} />
              ) : (
                <>
                  <div className="affected-files-list">
                    {suggestion.affected_file_ids.map((fid) => {
                      const file = fileCache[fid];
                      const isLoading = filesLoading[fid];

                      return (
                        <div key={fid} className="affected-file-row">
                          <FileText size={14} className="affected-file-icon" />
                          {isLoading ? (
                            <span className="affected-file-loading">Loading file #{fid}…</span>
                          ) : file ? (
                            <div className="affected-file-info">
                              <span className="affected-file-name">{file.file_name}</span>
                              <span className="affected-file-path" title={file.absolute_path}>
                                {file.absolute_path}
                              </span>
                              <span className="affected-file-size">
                                {formatBytes(file.size_bytes)}
                              </span>
                            </div>
                          ) : (
                            <span className="affected-file-name">Document #{fid}</span>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  {matchingGroup && (
                    <div style={{ marginTop: 10, display: "flex", justifyContent: "flex-end" }}>
                      <button
                        type="button"
                        className="btn btn-primary btn-sm"
                        onClick={() => setSelectedDuplicateGroupId(matchingGroup.id)}
                      >
                        <Copy size={13} /> Open Comparison & Mark Preferred
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>
      </div>
    );
  };

  // ── Render section helper ─────────────────────────────────────────────────
  const renderSection = (
    title: string,
    icon: React.ReactNode,
    description: string,
    items: OrganizationSuggestion[]
  ) => {
    // If a type filter is active and doesn't match, or if items are 0 when status/confidence filter applies
    if (typeFilter !== "all" && items.length === 0) {
      return null;
    }

    return (
      <section className="suggestion-section">
        <div className="suggestion-section__header">
          <div className="suggestion-section__title-group">
            {icon}
            <h3 className="suggestion-section__title">{title}</h3>
            <span className="badge badge-secondary">{items.length}</span>
          </div>
          <p className="suggestion-section__desc">{description}</p>
        </div>

        {items.length === 0 ? (
          <div className="suggestion-section__empty">
            <Info size={14} />
            <span>No {title.toLowerCase()} matching your current filters.</span>
          </div>
        ) : (
          <div className="suggestion-section__cards">
            {items.map(renderCard)}
          </div>
        )}
      </section>
    );
  };

  // ── Main UI ───────────────────────────────────────────────────────────────
  if (selectedDuplicateGroupId !== null) {
    return (
      <DuplicateGroupDetailView
        groupId={selectedDuplicateGroupId}
        onBack={() => {
          setSelectedDuplicateGroupId(null);
          loadSuggestions();
        }}
        onGroupChanged={() => {
          loadSuggestions();
          onSuggestionsChanged?.();
        }}
      />
    );
  }

  return (
    <div className="suggestions-view-container">
      {/* ── Top Bar / Header ───────────────────────────────────────────────── */}
      <div className="suggestions-header">
        <div className="suggestions-header__left">
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={onBackToLibrary}
            title="Return to library files view"
          >
            <ArrowLeft size={14} />
            <span>Back to Library</span>
          </button>
          <div>
            <div className="suggestions-header__title-row">
              <h2 className="suggestions-header__title">Organization Suggestions</h2>
              <span className="badge badge-warning">
                {pendingCount} Pending
              </span>
            </div>
            <p className="suggestions-header__subtitle">
              Locally analyzed patterns for collections, tags, versions, duplicates, and cleanup.
            </p>
          </div>
        </div>

        <div className="suggestions-header__right">
          <button
            type="button"
            className="btn btn-primary"
            onClick={handleAnalyzeLibrary}
            disabled={isAnalyzing || loading}
            title="Scan indexed metadata and regenerate local suggestions"
          >
            <Sparkles size={14} className={isAnalyzing ? "spin" : ""} />
            <span>{isAnalyzing ? "Analyzing…" : "Analyze Library"}</span>
          </button>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={loadSuggestions}
            disabled={loading}
            title="Refresh suggestions from database"
          >
            <RefreshCw size={14} className={loading ? "spin" : ""} />
          </button>
        </div>
      </div>

      {/* ── Strict Safety Guarantee Message ────────────────────────────────── */}
      <div className="safety-guarantee-banner" role="status">
        <ShieldCheck size={18} className="safety-guarantee-banner__icon" />
        <div className="safety-guarantee-banner__content">
          <strong>LibOne changes only its local metadata. Original files remain untouched.</strong>
          <p>
            Suggestions are never applied automatically. Approving a suggestion assigns tags,
            creates collections, or records duplicate references purely inside LibOne’s SQLite database.
            Original files on disk are never moved, renamed, altered, or deleted.
          </p>
        </div>
      </div>

      {/* ── Local AI Status Notice (Shows "Local AI is not configured" when unavailable) ── */}
      <div className="local-ai-status-notice" role="status">
        <Info size={15} className="local-ai-status-notice__icon" />
        <div className="local-ai-status-notice__content">
          <div className="local-ai-status-notice__header">
            <strong>{localAiStatus?.status_message || "Local AI is not configured"}</strong>
            <span className="badge badge-secondary local-ai-badge">Offline Metadata Only</span>
          </div>
          <p>
            Deterministic metadata suggestions remain active as the default engine.
            No cloud APIs, accounts, network requests, document uploads, or model downloads are used.
          </p>
        </div>
      </div>

      {/* ── Alerts: Error & Success Banners ─────────────────────────────────── */}
      {error && (
        <div className="alert-banner alert-banner--error" role="alert">
          <AlertCircle size={16} />
          <span className="alert-banner__message">{error}</span>
          <button
            type="button"
            className="btn-icon"
            onClick={() => setError(null)}
            title="Dismiss error"
          >
            <X size={14} />
          </button>
        </div>
      )}

      {actionSuccess && (
        <div className="alert-banner alert-banner--success" role="status">
          <CheckCircle2 size={16} />
          <span className="alert-banner__message">{actionSuccess}</span>
          <button
            type="button"
            className="btn-icon"
            onClick={() => setActionSuccess(null)}
            title="Dismiss notification"
          >
            <X size={14} />
          </button>
        </div>
      )}

      {/* ── Filter & Bulk Action Toolbar ────────────────────────────────────── */}
      <div className="suggestions-toolbar">
        <div className="suggestions-toolbar__filters">
          <div className="filter-group">
            <span className="filter-group__label">
              <Filter size={13} /> Type:
            </span>
            <select
              className="filter-select"
              value={typeFilter}
              onChange={(e) => setTypeFilter(e.target.value as TypeFilter)}
            >
              <option value="all">All Types</option>
              <option value="duplicate_files">Duplicates</option>
              <option value="likely_version">Likely Versions</option>
              <option value="suggest_collection">Collection Suggestions</option>
              <option value="suggest_tag">Tag Suggestions</option>
              <option value="stale_file">Stale Files</option>
            </select>
          </div>

          <div className="filter-group">
            <span className="filter-group__label">Confidence:</span>
            <select
              className="filter-select"
              value={confidenceFilter}
              onChange={(e) => setConfidenceFilter(e.target.value as ConfidenceFilter)}
            >
              <option value="all">All Levels</option>
              <option value="high">High Confidence (≥ 85%)</option>
              <option value="medium">Medium+ Confidence (≥ 75%)</option>
            </select>
          </div>

          <div className="filter-group">
            <span className="filter-group__label">Status:</span>
            <select
              className="filter-select"
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value as StatusFilter)}
            >
              <option value="pending">Pending</option>
              <option value="approved">Approved</option>
              <option value="rejected">Rejected</option>
              <option value="dismissed">Dismissed</option>
              <option value="all">All Statuses</option>
            </select>
          </div>
        </div>

        {/* Bulk Action Controls */}
        <div className="suggestions-toolbar__bulk">
          {filteredSuggestions.length > 0 && statusFilter === "pending" && (
            <>
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                onClick={handleSelectAllVisible}
              >
                {selectedIds.length === filteredSuggestions.length
                  ? "Deselect All"
                  : `Select All (${filteredSuggestions.length})`}
              </button>

              {selectedIds.length > 0 && (
                <>
                  <button
                    type="button"
                    className="btn btn-primary btn-sm"
                    onClick={handleBulkApprove}
                    disabled={loading}
                    title="Approve all selected suggestions"
                  >
                    <Check size={13} />
                    <span>Approve ({selectedIds.length})</span>
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={handleBulkReject}
                    disabled={loading}
                    title="Reject all selected suggestions"
                  >
                    <X size={13} />
                    <span>Reject ({selectedIds.length})</span>
                  </button>
                </>
              )}
            </>
          )}
        </div>
      </div>

      {/* ── Content View: Loading, Empty, or Categorized Sections ───────────── */}
      {loading ? (
        <div className="suggestions-loading-state">
          <RefreshCw size={24} className="spin" />
          <p>Loading suggestions…</p>
        </div>
      ) : filteredSuggestions.length === 0 ? (
        <div className="suggestions-empty-state">
          <Sparkles size={36} className="empty-state-icon" />
          <h3>No suggestions found</h3>
          <p>
            {statusFilter === "pending"
              ? "All pending suggestions have been reviewed, or no patterns met the active filter criteria."
              : `No suggestions matching status '${statusFilter}' and the selected filters.`}
          </p>
          <div className="empty-state-actions">
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleAnalyzeLibrary}
              disabled={isAnalyzing}
            >
              <Sparkles size={14} className={isAnalyzing ? "spin" : ""} />
              <span>{isAnalyzing ? "Analyzing…" : "Run Library Analysis"}</span>
            </button>
          </div>
        </div>
      ) : (
        <div className="suggestions-sections-container">
          {/* 1. Duplicate Files Section */}
          {renderSection(
            "Duplicate Files",
            <Copy size={16} className="section-icon--duplicates" />,
            "Identical NTFS identities, exact content hashes, or matching filenames and byte sizes.",
            duplicateSuggestions
          )}

          {/* 2. Likely Versions Section */}
          {renderSection(
            "Likely Versions",
            <Clock size={16} className="section-icon--versions" />,
            "Documents in the same directory sharing base stems with version suffixes like v1, v2, final, draft.",
            versionSuggestions
          )}

          {/* 3. Collection Suggestions Section */}
          {renderSection(
            "Collection Suggestions",
            <FolderPlus size={16} className="section-icon--collections" />,
            "Folder clusters sharing project or topic paths suitable for grouping into collections.",
            collectionSuggestions
          )}

          {/* 4. Tag Suggestions Section */}
          {renderSection(
            "Tag Suggestions",
            <TagIcon size={16} className="section-icon--tags" />,
            "Semantic keywords, 4-digit year tokens, and document formats extracted from indexed metadata.",
            tagSuggestions
          )}

          {/* 5. Stale-File Suggestions Section */}
          {renderSection(
            "Stale-File Suggestions",
            <AlertTriangle size={16} className="section-icon--stale" />,
            "Missing disk records or inactive documents unmodified for over the configured threshold.",
            staleSuggestions
          )}
        </div>
      )}
    </div>
  );
};
