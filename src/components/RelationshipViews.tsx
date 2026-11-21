import React, { useState, useEffect, useMemo } from "react";
import {
  AlertTriangle,
  Clock,
  FileText,
  Info,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { FileComparisonItem, OrganizationSuggestion } from "../types";
import { getFileComparison } from "../services/api";

// ── Version Group Display ──────────────────────────────────────────────────────

interface VersionGroupProps {
  suggestion: OrganizationSuggestion;
}

export const VersionGroupDisplay: React.FC<VersionGroupProps> = ({ suggestion }) => {
  const [files, setFiles] = useState<FileComparisonItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      setError(null);
      try {
        const items = await getFileComparison(suggestion.affected_file_ids);
        setFiles(items);
      } catch (e: any) {
        setError(e?.message || String(e));
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [suggestion.id]);

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  const formatDate = (iso: string | null): string => {
    if (!iso) return "—";
    try {
      return new Date(iso).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return iso;
    }
  };

  // Sort by modified date descending (newest first = most recent version)
  const sortedFiles = useMemo(() => {
    return [...files].sort((a, b) => {
      const dateA = a.file.modified_at || a.file.discovered_at;
      const dateB = b.file.modified_at || b.file.discovered_at;
      return dateB.localeCompare(dateA);
    });
  }, [files]);

  if (loading) {
    return (
      <div className="version-group__loading">
        <RefreshCw size={14} className="spin" />
        <span>Loading version details…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="version-group__error">
        <AlertTriangle size={14} />
        <span>{error}</span>
      </div>
    );
  }

  return (
    <div className="version-group-display">
      <div className="version-group__header">
        <Clock size={14} />
        <span className="version-group__label">
          {sortedFiles.length} related version{sortedFiles.length !== 1 ? "s" : ""} detected
        </span>
      </div>

      <div className="version-group__timeline">
        {sortedFiles.map((item, idx) => {
          const warnings: string[] = [];
          if (!item.exists_on_disk) warnings.push("Not on disk");
          if (item.file.status === "missing") warnings.push("Missing");
          if (item.file.status === "inaccessible") warnings.push("Inaccessible");
          if (!item.root_enabled) warnings.push("Root disabled");

          return (
            <div
              key={item.file.id}
              className={`version-item ${
                warnings.length > 0 ? "version-item--warning" : ""
              } ${idx === 0 ? "version-item--latest" : ""}`}
            >
              <div className="version-item__indicator">
                <div className="version-item__dot" />
                {idx < sortedFiles.length - 1 && <div className="version-item__line" />}
              </div>
              <div className="version-item__content">
                <div className="version-item__name">
                  <FileText size={13} />
                  <span>{item.file.file_name}</span>
                  {idx === 0 && (
                    <span className="badge badge-success version-badge">Latest</span>
                  )}
                  {warnings.length > 0 && (
                    <span className="badge badge-warning version-badge">
                      <AlertTriangle size={10} /> {warnings.join(", ")}
                    </span>
                  )}
                </div>
                <div className="version-item__details">
                  <span title={item.file.absolute_path}>{item.file.absolute_path}</span>
                  <span className="version-item__sep">•</span>
                  <span>{formatBytes(item.file.size_bytes)}</span>
                  <span className="version-item__sep">•</span>
                  <span>Modified: {formatDate(item.file.modified_at)}</span>
                  {item.file.content_hash_nullable && (
                    <>
                      <span className="version-item__sep">•</span>
                      <span
                        className="version-item__hash"
                        title={item.file.content_hash_nullable}
                      >
                        Hash: {item.file.content_hash_nullable.substring(0, 10)}…
                      </span>
                    </>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      <div className="version-group__explanation">
        <Info size={12} />
        <span>
          Versions are grouped by shared filename stems with version suffixes (v1, v2, final,
          draft) in the same directory. Approving tags them with "version-series" in metadata only.
        </span>
      </div>
    </div>
  );
};

// ── Stale File Explanation ──────────────────────────────────────────────────────

interface StaleFileProps {
  suggestion: OrganizationSuggestion;
}

export const StaleFileExplanation: React.FC<StaleFileProps> = ({ suggestion }) => {
  const [files, setFiles] = useState<FileComparisonItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      setError(null);
      try {
        const items = await getFileComparison(suggestion.affected_file_ids);
        setFiles(items);
      } catch (e: any) {
        setError(e?.message || String(e));
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [suggestion.id]);

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  const formatDate = (iso: string | null): string => {
    if (!iso) return "—";
    try {
      return new Date(iso).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return iso;
    }
  };

  const daysSince = (iso: string | null): string => {
    if (!iso) return "unknown";
    try {
      const diff = Date.now() - new Date(iso).getTime();
      const days = Math.floor(diff / (1000 * 60 * 60 * 24));
      if (days < 1) return "today";
      if (days === 1) return "1 day ago";
      return `${days} days ago`;
    } catch {
      return "unknown";
    }
  };

  if (loading) {
    return (
      <div className="stale-files__loading">
        <RefreshCw size={14} className="spin" />
        <span>Loading stale file details…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="stale-files__error">
        <AlertTriangle size={14} />
        <span>{error}</span>
      </div>
    );
  }

  const isMissingSuggestion = suggestion.title.toLowerCase().includes("missing");

  return (
    <div className="stale-files-display">
      <div className="stale-files__header">
        <AlertTriangle size={14} />
        <span className="stale-files__label">
          {files.length} stale {isMissingSuggestion ? "missing" : "inactive"} file
          {files.length !== 1 ? "s" : ""}
        </span>
      </div>

      <div className="stale-files__explanation-box">
        <ShieldCheck size={14} />
        <div>
          <strong>
            {isMissingSuggestion
              ? "These files are no longer found on disk."
              : "These files have not been modified for an extended period."}
          </strong>
          <p>
            {isMissingSuggestion
              ? "Approving removes their index records from LibOne's database. The original files (if they still exist elsewhere) are never deleted."
              : "Approving tags them with 'archive' in LibOne metadata for organizational purposes. No files are moved, deleted, or altered."}
          </p>
        </div>
      </div>

      <div className="stale-files__list">
        {files.map((item) => {
          const warnings: string[] = [];
          if (!item.exists_on_disk) warnings.push("Not on disk");
          if (item.file.status === "missing") warnings.push("Missing");
          if (item.file.status === "inaccessible") warnings.push("Inaccessible");
          if (!item.root_enabled) warnings.push("Root disabled");

          return (
            <div
              key={item.file.id}
              className={`stale-file-item ${
                warnings.length > 0 ? "stale-file-item--warning" : ""
              }`}
            >
              <div className="stale-file-item__icon">
                <FileText size={14} />
              </div>
              <div className="stale-file-item__content">
                <div className="stale-file-item__name">
                  <span>{item.file.file_name}</span>
                  {warnings.map((w, i) => (
                    <span key={i} className="badge badge-warning stale-badge">
                      <AlertTriangle size={10} /> {w}
                    </span>
                  ))}
                </div>
                <div className="stale-file-item__details">
                  <span className="stale-file-item__path" title={item.file.absolute_path}>
                    {item.file.absolute_path}
                  </span>
                </div>
                <div className="stale-file-item__meta">
                  <span>Size: {formatBytes(item.file.size_bytes)}</span>
                  <span className="stale-sep">•</span>
                  <span>Last modified: {formatDate(item.file.modified_at)}</span>
                  <span className="stale-sep">•</span>
                  <span>Last seen: {formatDate(item.file.last_seen_at)} ({daysSince(item.file.last_seen_at)})</span>
                  <span className="stale-sep">•</span>
                  <span>
                    Root: {item.root_display_name}
                    {!item.root_enabled && <span className="root-disabled-inline"> (disabled)</span>}
                  </span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
