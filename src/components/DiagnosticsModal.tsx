import React, { useEffect, useState } from "react";
import {
  Activity,
  AlertCircle,
  AlertTriangle,
  Check,
  CheckCircle2,
  Copy,
  Database,
  Download,
  Eye,
  EyeOff,
  HardDrive,
  HelpCircle,
  Radio,
  RefreshCw,
  Save,
  ShieldCheck,
  X,
} from "lucide-react";
import { DiagnosticsReport } from "../types";
import {
  getSystemDiagnostics,
  promptSaveDiagnosticsPath,
  saveDiagnosticsReport,
} from "../services/api";

interface DiagnosticsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function redactDiagnosticsReport(report: DiagnosticsReport): DiagnosticsReport {
  const redactPath = (path: string): string => {
    if (!path) return path;
    // Replace Windows C:\Users\Username with C:\Users\<redacted>
    let sanitized = path.replace(/([a-zA-Z]:\\Users\\)[^\\]+/gi, "$1<redacted>");
    // Replace Unix /Users/username or /home/username with /home/<redacted>
    sanitized = sanitized.replace(/(\/(?:home|Users)\/)[^\/]+/gi, "$1<redacted>");
    return sanitized;
  };

  return {
    ...report,
    db_path: redactPath(report.db_path),
    app_data_dir: redactPath(report.app_data_dir),
    backup_health: {
      ...report.backup_health,
      latest_safety_backup_path: report.backup_health.latest_safety_backup_path
        ? redactPath(report.backup_health.latest_safety_backup_path)
        : null,
    },
    watcher_health: {
      ...report.watcher_health,
      root_statuses: report.watcher_health.root_statuses.map((r) => ({
        ...r,
        path: redactPath(r.path),
      })),
    },
  };
}

export function formatDiagnosticsAsText(report: DiagnosticsReport, redacted: boolean): string {
  const r = redacted ? redactDiagnosticsReport(report) : report;
  const lines: string[] = [];
  lines.push("================================================================================");
  lines.push(`LibOne System Diagnostics ${redacted ? "(REDACTED FOR PRIVACY)" : "(LOCAL FULL)"}`);
  lines.push(`Generated: ${r.generated_at}`);
  lines.push("Privacy Guarantee: Contains filesystem paths and metadata counters only.");
  lines.push("No document contents, note texts, tokens, or credentials are included.");
  lines.push("================================================================================");
  lines.push("");
  lines.push("--- Application & Environment ---");
  lines.push(`App Version:          v${r.app_version}`);
  lines.push(`Schema Version:       v${r.schema_version}`);
  lines.push(`Operating System:     ${r.os}`);
  lines.push(`Database Path:        ${r.db_path}`);
  lines.push(`App Data Directory:   ${r.app_data_dir}`);
  lines.push("");
  lines.push("--- Storage Volume & Disk Health ---");
  if (r.disk_space) {
    const freeGb = (r.disk_space.available_bytes / (1024 * 1024 * 1024)).toFixed(2);
    const totalGb = (r.disk_space.total_bytes / (1024 * 1024 * 1024)).toFixed(2);
    lines.push(`Available Disk Space: ${freeGb} GB / ${totalGb} GB (${r.disk_space.free_percentage.toFixed(1)}% free)`);
    lines.push(`Low Space Warning:    ${r.disk_space.is_low_space ? "YES - " + (r.disk_space.warning_message ?? "") : "NO (Healthy)"}`);
  } else {
    lines.push("Disk Space:           Not safely detectable on current platform");
  }
  lines.push("");
  lines.push("--- Roots & Scanning Health ---");
  lines.push(`Enabled Scan Roots:   ${r.scan_health.enabled_roots_count}`);
  lines.push(`Disabled Scan Roots:  ${r.scan_health.disabled_roots_count}`);
  lines.push(`Total Scan Roots:     ${r.scan_health.total_roots_count}`);
  lines.push(`Last Scan Status:     ${r.scan_health.last_scan_status ?? "Never Scanned"}`);
  lines.push(`Last Scan Started:    ${r.scan_health.last_scan_started_at ?? "Never"}`);
  lines.push(`Last Scan Completed:  ${r.scan_health.last_scan_completed_at ?? "Never"}`);
  lines.push("");
  lines.push("--- File Index Counts ---");
  lines.push(`Indexed Files:        ${r.file_counts.indexed_files_count}`);
  lines.push(`Missing Files:        ${r.file_counts.missing_files_count}`);
  lines.push(`Inaccessible Files:   ${r.file_counts.inaccessible_files_count}`);
  lines.push(`Error/Other Status:   ${r.file_counts.error_files_count}`);
  lines.push(`Total Tracked Files:  ${r.file_counts.total_tracked_files}`);
  lines.push("");
  lines.push("--- File Watcher Health ---");
  lines.push(`Watching Active:      ${r.watcher_health.is_watching ? "YES (Active)" : "NO (Idle/Stopped)"}`);
  lines.push(`Watched Roots Count:  ${r.watcher_health.watched_roots_count}`);
  lines.push(`Coalescing Burst:     ${r.watcher_health.is_coalescing ? "YES" : "NO"}`);
  lines.push(`Last Refreshed:       ${r.watcher_health.last_refreshed_at ?? "None"}`);
  if (r.watcher_health.root_statuses.length > 0) {
    lines.push("Root Watchers Detail:");
    r.watcher_health.root_statuses.forEach((rw) => {
      lines.push(
        `  - [Root #${rw.root_id}] ${rw.path} (active: ${rw.active}${rw.error_message ? ", error: " + rw.error_message : ""})`
      );
    });
  }
  lines.push("");
  lines.push("--- Backup & Restore Health ---");
  lines.push(`Backup Status:        ${r.backup_health.status}`);
  lines.push(`Safety Backups Count: ${r.backup_health.safety_backups_count}`);
  lines.push(`Latest Safety Backup: ${r.backup_health.latest_safety_backup_time ?? "None"}`);
  if (r.backup_health.latest_safety_backup_path) {
    lines.push(`Safety Backup Path:   ${r.backup_health.latest_safety_backup_path}`);
  }
  lines.push("");
  lines.push("--- Database Integrity Report ---");
  lines.push(`Integrity Check:      ${r.integrity_report.integrity_check_passed ? "PASSED (ok)" : "FAILED"}`);
  lines.push(`Foreign Keys Check:   ${r.integrity_report.foreign_key_check_passed ? "PASSED (0 violations)" : `FAILED (${r.integrity_report.foreign_key_violations.length} violations)`}`);
  lines.push(`Missing Tables:       ${r.integrity_report.missing_tables.length === 0 ? "None (0)" : r.integrity_report.missing_tables.join(", ")}`);
  lines.push(`Invalid Paths Count:  ${r.integrity_report.invalid_paths_count}`);
  lines.push(`Missing Files Count:  ${r.integrity_report.missing_files_count}`);
  lines.push(`Total Issues Logged:  ${r.integrity_report.issues.length}`);
  if (r.integrity_report.issues.length > 0) {
    lines.push("Integrity Diagnostics Detail:");
    r.integrity_report.issues.forEach((iss, i) => {
      lines.push(`  [${i + 1}] [${iss.severity.toUpperCase()}] (${iss.category}) ${iss.message}`);
      lines.push(`      Repair: ${iss.repair_guidance}`);
    });
  }
  lines.push("================================================================================");
  return lines.join("\n");
}

export const DiagnosticsModal: React.FC<DiagnosticsModalProps> = ({
  isOpen,
  onClose,
}) => {
  const [report, setReport] = useState<DiagnosticsReport | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<boolean>(false);
  const [redactPaths, setRedactPaths] = useState<boolean>(true);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  const fetchDiagnostics = async () => {
    setLoading(true);
    setError(null);
    setStatusMessage(null);
    try {
      const data = await getSystemDiagnostics();
      setReport(data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen) {
      fetchDiagnostics();
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleCopy = async (redacted: boolean) => {
    if (!report) return;
    const text = formatDiagnosticsAsText(report, redacted);
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setStatusMessage(redacted ? "Copied redacted diagnostics to clipboard." : "Copied full diagnostics to clipboard.");
      setTimeout(() => setCopied(false), 2500);
    } catch (e) {
      console.warn("Failed to copy diagnostics:", e);
      setStatusMessage("Failed to copy to clipboard.");
    }
  };

  const handleSaveFile = async (format: "json" | "txt") => {
    if (!report) return;
    try {
      const selectedPath = await promptSaveDiagnosticsPath(format);
      if (!selectedPath) return; // User cancelled

      const content =
        format === "json"
          ? JSON.stringify(redactPaths ? redactDiagnosticsReport(report) : report, null, 2)
          : formatDiagnosticsAsText(report, redactPaths);

      await saveDiagnosticsReport(selectedPath, content);
      setStatusMessage(`Diagnostics saved successfully to ${selectedPath}`);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal-content"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 780, width: "92%", maxHeight: "90vh", display: "flex", flexDirection: "column" }}
      >
        {/* Header */}
        <div className="modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Activity size={20} color="#38bdf8" />
            <h3 style={{ fontSize: "1.1rem", margin: 0 }}>System Diagnostics & Library Health</h3>
          </div>
          <button className="btn btn-danger-ghost" onClick={onClose}>
            <X size={18} />
          </button>
        </div>

        {/* Scrollable Body */}
        <div className="modal-body" style={{ overflowY: "auto", padding: "16px 20px", display: "flex", flexDirection: "column", gap: 14 }}>
          {/* Privacy Guarantee Disclosure Banner */}
          <div
            style={{
              padding: "10px 14px",
              borderRadius: "var(--radius-sm)",
              background: "rgba(52, 211, 153, 0.08)",
              border: "1px solid rgba(52, 211, 153, 0.25)",
              display: "flex",
              alignItems: "flex-start",
              gap: 10,
            }}
          >
            <ShieldCheck size={18} color="#34d399" style={{ flexShrink: 0, marginTop: 2 }} />
            <div style={{ fontSize: "0.78rem", color: "var(--text-secondary)", lineHeight: 1.45 }}>
              <strong style={{ color: "#34d399" }}>Zero-Cloud Privacy Guarantee:</strong> Diagnostics examine local
              database metrics, filesystem counts, and process status only. Document contents, notes, and credentials
              are <strong>never</strong> collected, transmitted, or uploaded.
            </div>
          </div>

          {loading && (
            <div style={{ textAlign: "center", padding: "40px 0", color: "var(--text-muted)", fontSize: "0.85rem" }}>
              <RefreshCw size={24} className="spin" style={{ margin: "0 auto 8px auto", display: "block" }} />
              Gathering local system diagnostics and database health metrics…
            </div>
          )}

          {error && (
            <div
              style={{
                padding: "10px 12px",
                borderRadius: "var(--radius-sm)",
                background: "rgba(248, 113, 113, 0.1)",
                border: "1px solid rgba(248, 113, 113, 0.3)",
                color: "#f87171",
                fontSize: "0.82rem",
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <AlertCircle size={16} />
              <span>Failed to load diagnostics: {error}</span>
            </div>
          )}

          {statusMessage && (
            <div
              style={{
                padding: "8px 12px",
                borderRadius: "var(--radius-sm)",
                background: "rgba(56, 189, 248, 0.1)",
                border: "1px solid rgba(56, 189, 248, 0.3)",
                color: "#38bdf8",
                fontSize: "0.78rem",
              }}
            >
              {statusMessage}
            </div>
          )}

          {report && !loading && (
            <>
              {/* Disk Space Warning Alert (if low) */}
              {report.disk_space?.is_low_space && (
                <div
                  style={{
                    padding: "10px 14px",
                    borderRadius: "var(--radius-sm)",
                    background: "rgba(245, 158, 11, 0.12)",
                    border: "1px solid rgba(245, 158, 11, 0.35)",
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                  }}
                >
                  <AlertTriangle size={18} color="#f59e0b" style={{ flexShrink: 0 }} />
                  <div style={{ fontSize: "0.8rem", color: "#f59e0b", fontWeight: 600 }}>
                    {report.disk_space.warning_message ?? "Low available disk space detected on library drive."}
                  </div>
                </div>
              )}

              {/* Grid 1: System, DB & Disk Health */}
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                {/* System & Application */}
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 600, fontSize: "0.82rem", color: "#38bdf8" }}>
                    <Database size={14} /> System & Database
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">LibOne Version</span>
                    <span className="settings-kv-value">v{report.app_version}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Schema Version</span>
                    <span className="settings-kv-value">v{report.schema_version}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Operating System</span>
                    <span className="settings-kv-value">{report.os}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Database Path</span>
                    <span className="settings-kv-value" title={report.db_path}>
                      {redactPaths ? redactDiagnosticsReport(report).db_path : report.db_path}
                    </span>
                  </div>
                </div>

                {/* Disk Space Health */}
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 600, fontSize: "0.82rem", color: "#38bdf8" }}>
                    <HardDrive size={14} /> Storage Volume
                  </div>
                  {report.disk_space ? (
                    <>
                      <div className="settings-kv-row">
                        <span className="settings-kv-label">Available Free</span>
                        <span className="settings-kv-value" style={{ color: report.disk_space.is_low_space ? "#f59e0b" : "#34d399" }}>
                          {(report.disk_space.available_bytes / (1024 * 1024 * 1024)).toFixed(2)} GB ({report.disk_space.free_percentage.toFixed(1)}%)
                        </span>
                      </div>
                      <div className="settings-kv-row">
                        <span className="settings-kv-label">Total Capacity</span>
                        <span className="settings-kv-value">
                          {(report.disk_space.total_bytes / (1024 * 1024 * 1024)).toFixed(2)} GB
                        </span>
                      </div>
                      <div className="settings-kv-row">
                        <span className="settings-kv-label">Space Warning</span>
                        <span className="settings-kv-value" style={{ color: report.disk_space.is_low_space ? "#f59e0b" : "#34d399" }}>
                          {report.disk_space.is_low_space ? "Low Free Space" : "Sufficient Space (Healthy)"}
                        </span>
                      </div>
                    </>
                  ) : (
                    <div style={{ fontSize: "0.78rem", color: "var(--text-muted)" }}>
                      Disk space safely undetectable on this platform
                    </div>
                  )}
                </div>
              </div>

              {/* Grid 2: Roots & Index Counts */}
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                {/* Scan Roots & Status */}
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 600, fontSize: "0.82rem", color: "#38bdf8" }}>
                    <Activity size={14} /> Roots & Scanner
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Enabled Roots</span>
                    <span className="settings-kv-value" style={{ color: "#34d399" }}>{report.scan_health.enabled_roots_count}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Disabled Roots</span>
                    <span className="settings-kv-value">{report.scan_health.disabled_roots_count}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Last Scan Status</span>
                    <span className="settings-kv-value">{report.scan_health.last_scan_status ?? "Never Scanned"}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Last Scan Completed</span>
                    <span className="settings-kv-value">
                      {report.scan_health.last_scan_completed_at
                        ? new Date(report.scan_health.last_scan_completed_at).toLocaleString()
                        : "Never"}
                    </span>
                  </div>
                </div>

                {/* Indexed Files Breakdown */}
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 600, fontSize: "0.82rem", color: "#38bdf8" }}>
                    <CheckCircle2 size={14} /> Indexed Files Health
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Indexed (Healthy)</span>
                    <span className="settings-kv-value" style={{ color: "#34d399" }}>
                      {report.file_counts.indexed_files_count}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Missing on Disk</span>
                    <span className="settings-kv-value" style={{ color: report.file_counts.missing_files_count > 0 ? "#f59e0b" : "#34d399" }}>
                      {report.file_counts.missing_files_count}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Inaccessible Files</span>
                    <span className="settings-kv-value" style={{ color: report.file_counts.inaccessible_files_count > 0 ? "#f87171" : "#34d399" }}>
                      {report.file_counts.inaccessible_files_count}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Total Tracked Files</span>
                    <span className="settings-kv-value">{report.file_counts.total_tracked_files}</span>
                  </div>
                </div>
              </div>

              {/* Grid 3: Watcher & Backup Health */}
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                {/* Watcher Health */}
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 600, fontSize: "0.82rem", color: "#38bdf8" }}>
                    <Radio size={14} /> Real-Time Watcher Health
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Watching Active</span>
                    <span
                      className="settings-kv-value"
                      style={{
                        color: report.watcher_health.is_watching ? "#34d399" : "var(--text-muted)",
                        fontWeight: 600,
                      }}
                    >
                      {report.watcher_health.is_watching ? "ACTIVE" : "IDLE / STOPPED"}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Watched Roots</span>
                    <span className="settings-kv-value">{report.watcher_health.watched_roots_count} roots</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Activity Status</span>
                    <span className="settings-kv-value">
                      {report.watcher_health.is_refreshing
                        ? "Refreshing..."
                        : report.watcher_health.is_coalescing
                        ? "Debouncing bursts"
                        : "Idle (listening)"}
                    </span>
                  </div>
                </div>

                {/* Backup & Safety Recovery */}
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 8 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 600, fontSize: "0.82rem", color: "#38bdf8" }}>
                    <Save size={14} /> Backup & Safety Health
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Status</span>
                    <span className="settings-kv-value" style={{ color: "#34d399" }}>{report.backup_health.status}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Pre-Restore Snapshots</span>
                    <span className="settings-kv-value">{report.backup_health.safety_backups_count} snapshot(s)</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Latest Safety Snapshot</span>
                    <span className="settings-kv-value">
                      {report.backup_health.latest_safety_backup_time
                        ? new Date(report.backup_health.latest_safety_backup_time).toLocaleString()
                        : "None"}
                    </span>
                  </div>
                </div>
              </div>

              {/* Database Integrity Diagnostic Summary */}
              <div className="settings-info-card" style={{ flexDirection: "column", gap: 10 }}>
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 600, fontSize: "0.82rem", color: "#38bdf8" }}>
                    <ShieldCheck size={14} /> Database Integrity Result
                  </div>
                  <span
                    style={{
                      fontSize: "0.75rem",
                      fontWeight: 600,
                      padding: "2px 8px",
                      borderRadius: 4,
                      background: report.integrity_report.is_healthy ? "rgba(52, 211, 153, 0.15)" : "rgba(245, 158, 11, 0.15)",
                      color: report.integrity_report.is_healthy ? "#34d399" : "#f59e0b",
                    }}
                  >
                    {report.integrity_report.is_healthy ? "INTEGRITY OK" : "ISSUES LOGGED"}
                  </span>
                </div>

                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "6px 20px" }}>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">PRAGMA Quick Check</span>
                    <span className="settings-kv-value" style={{ color: report.integrity_report.integrity_check_passed ? "#34d399" : "#f87171" }}>
                      {report.integrity_report.integrity_check_passed ? "Passed (ok)" : "Failed"}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Foreign Keys</span>
                    <span className="settings-kv-value" style={{ color: report.integrity_report.foreign_key_check_passed ? "#34d399" : "#f87171" }}>
                      {report.integrity_report.foreign_key_violations.length === 0 ? "0 violations" : `${report.integrity_report.foreign_key_violations.length} violations`}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Missing Tables</span>
                    <span className="settings-kv-value">
                      {report.integrity_report.missing_tables.length === 0 ? "None (all present)" : report.integrity_report.missing_tables.join(", ")}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Invalid Paths</span>
                    <span className="settings-kv-value">{report.integrity_report.invalid_paths_count}</span>
                  </div>
                </div>

                {report.integrity_report.issues.length > 0 && (
                  <div style={{ marginTop: 6, display: "flex", flexDirection: "column", gap: 6 }}>
                    <span style={{ fontSize: "0.76rem", fontWeight: 600 }}>Actionable Guidance:</span>
                    {report.integrity_report.issues.map((iss, i) => (
                      <div
                        key={i}
                        style={{
                          padding: "6px 10px",
                          borderRadius: 4,
                          background: "var(--bg-panel)",
                          fontSize: "0.76rem",
                          borderLeft: `3px solid ${iss.severity === "error" ? "#f87171" : "#f59e0b"}`,
                        }}
                      >
                        <div style={{ fontWeight: 600 }}>[{iss.category}] {iss.message}</div>
                        <div style={{ color: "var(--text-secondary)", marginTop: 2, display: "flex", alignItems: "center", gap: 4 }}>
                          <HelpCircle size={12} color="#38bdf8" /> {iss.repair_guidance}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </>
          )}
        </div>

        {/* Footer Actions */}
        <div
          className="modal-footer"
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "12px 20px",
            borderTop: "1px solid var(--border-color)",
            backgroundColor: "var(--bg-surface)",
          }}
        >
          {/* Left: Redact Toggle & Re-run */}
          <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
            <label
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                fontSize: "0.78rem",
                color: "var(--text-secondary)",
                cursor: "pointer",
              }}
              title="Redact usernames and home directory names in copied or saved diagnostics"
            >
              <input
                type="checkbox"
                checked={redactPaths}
                onChange={(e) => setRedactPaths(e.target.checked)}
                style={{ cursor: "pointer" }}
              />
              {redactPaths ? <EyeOff size={13} color="#34d399" /> : <Eye size={13} color="#f59e0b" />}
              <span>Redact user paths</span>
            </label>

            <button
              className="btn btn-secondary"
              onClick={fetchDiagnostics}
              disabled={loading}
              style={{ fontSize: "0.76rem", padding: "5px 10px" }}
              title="Refresh diagnostics metrics"
            >
              <RefreshCw size={12} className={loading ? "spin" : ""} /> Refresh
            </button>
          </div>

          {/* Right: Copy & Save actions */}
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button
              className="btn btn-secondary"
              onClick={() => handleCopy(redactPaths)}
              disabled={!report || loading}
              style={{ fontSize: "0.76rem", padding: "5px 12px" }}
              title="Copy diagnostics text to clipboard"
            >
              {copied ? <Check size={13} color="#34d399" /> : <Copy size={13} />}
              {copied ? "Copied!" : redactPaths ? "Copy (Redacted)" : "Copy Text"}
            </button>

            <button
              className="btn btn-secondary"
              onClick={() => handleSaveFile("txt")}
              disabled={!report || loading}
              style={{ fontSize: "0.76rem", padding: "5px 12px" }}
              title="Save diagnostics as formatted .txt report"
            >
              <Save size={13} /> Save .txt
            </button>

            <button
              className="btn btn-secondary"
              onClick={() => handleSaveFile("json")}
              disabled={!report || loading}
              style={{ fontSize: "0.76rem", padding: "5px 12px" }}
              title="Save diagnostics as portable .json file"
            >
              <Download size={13} /> Save .json
            </button>

            <button className="btn btn-primary" onClick={onClose} style={{ fontSize: "0.76rem", padding: "5px 14px" }}>
              Done
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
