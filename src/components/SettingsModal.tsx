import React, { useState } from "react";
import {
  Settings,
  X,
  Database,
  HardDrive,
  ShieldCheck,
  Download,
  FolderLock,
  CloudOff,
  FileCode,
  Save,
  Upload,
  AlertTriangle,
  CheckCircle2,
  RefreshCw,
  Activity,
  AlertCircle,
  HelpCircle,
} from "lucide-react";
import {
  AppInfo,
  BackupResult,
  BackupValidationResult,
  DatabaseIntegrityReport,
  RestoreResult,
} from "../types";
import {
  checkDatabaseIntegrity,
  createDatabaseBackup,
  restoreDatabase,
  selectBackupOpenPath,
  selectBackupSavePath,
  validateBackup,
} from "../services/api";

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  appInfo: AppInfo | null;
  onOpenExport: () => void;
  onLibraryRestored?: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  appInfo,
  onOpenExport,
  onLibraryRestored,
}) => {
  const [activeTab, setActiveTab] = useState<"storage" | "backup" | "integrity">("storage");

  // Backup state
  const [backupLoading, setBackupLoading] = useState(false);
  const [backupSuccess, setBackupSuccess] = useState<BackupResult | null>(null);
  const [backupError, setBackupError] = useState<string | null>(null);

  // Restore state
  const [pendingRestoreValidation, setPendingRestoreValidation] = useState<BackupValidationResult | null>(null);
  const [pendingRestorePath, setPendingRestorePath] = useState<string | null>(null);
  const [restoreLoading, setRestoreLoading] = useState(false);
  const [restoreSuccess, setRestoreSuccess] = useState<RestoreResult | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);

  // Integrity check state
  const [integrityLoading, setIntegrityLoading] = useState(false);
  const [integrityReport, setIntegrityReport] = useState<DatabaseIntegrityReport | null>(null);
  const [integrityError, setIntegrityError] = useState<string | null>(null);

  if (!isOpen) return null;

  // Handle Backup
  const handleCreateBackup = async () => {
    setBackupError(null);
    setBackupSuccess(null);
    try {
      const selectedPath = await selectBackupSavePath();
      if (!selectedPath) return; // user cancelled

      setBackupLoading(true);
      const res = await createDatabaseBackup(selectedPath);
      setBackupSuccess(res);
    } catch (err: unknown) {
      setBackupError(err instanceof Error ? err.message : String(err));
    } finally {
      setBackupLoading(false);
    }
  };

  // Handle Select Restore File
  const handleSelectRestore = async () => {
    setRestoreError(null);
    setRestoreSuccess(null);
    setPendingRestoreValidation(null);
    setPendingRestorePath(null);

    try {
      const selectedPath = await selectBackupOpenPath();
      if (!selectedPath) return; // user cancelled

      setRestoreLoading(true);
      const validation = await validateBackup(selectedPath);
      if (!validation.is_valid) {
        setRestoreError(`Invalid backup: ${validation.warnings.join(", ") || "Validation failed"}`);
        return;
      }

      setPendingRestorePath(selectedPath);
      setPendingRestoreValidation(validation);
    } catch (err: unknown) {
      setRestoreError(err instanceof Error ? err.message : String(err));
    } finally {
      setRestoreLoading(false);
    }
  };

  // Handle Execute Confirmed Restore
  const handleConfirmRestore = async () => {
    if (!pendingRestorePath) return;
    setRestoreLoading(true);
    setRestoreError(null);

    try {
      const res = await restoreDatabase(pendingRestorePath);
      setRestoreSuccess(res);
      setPendingRestorePath(null);
      setPendingRestoreValidation(null);
      if (onLibraryRestored) {
        onLibraryRestored();
      }
    } catch (err: unknown) {
      setRestoreError(err instanceof Error ? err.message : String(err));
    } finally {
      setRestoreLoading(false);
    }
  };

  // Handle Integrity Check
  const handleRunIntegrityCheck = async () => {
    setIntegrityLoading(true);
    setIntegrityError(null);
    try {
      const report = await checkDatabaseIntegrity();
      setIntegrityReport(report);
    } catch (err: unknown) {
      setIntegrityError(err instanceof Error ? err.message : String(err));
    } finally {
      setIntegrityLoading(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal-content settings-modal-content"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 720 }}
      >
        <div className="modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Settings size={20} color="#38bdf8" />
            <h3 style={{ fontSize: "1.1rem" }}>Settings & Library Maintenance</h3>
          </div>
          <button className="btn btn-danger-ghost" onClick={onClose}>
            <X size={18} />
          </button>
        </div>

        {/* Tab switcher */}
        <div
          style={{
            display: "flex",
            gap: 8,
            padding: "8px 24px",
            borderBottom: "1px solid var(--border-color)",
            backgroundColor: "rgba(0,0,0,0.15)",
          }}
        >
          <button
            className={`btn ${activeTab === "storage" ? "btn-primary" : "btn-secondary"}`}
            style={{ fontSize: "0.82rem", padding: "6px 14px" }}
            onClick={() => setActiveTab("storage")}
          >
            <Database size={14} /> Overview & Storage
          </button>
          <button
            className={`btn ${activeTab === "backup" ? "btn-primary" : "btn-secondary"}`}
            style={{ fontSize: "0.82rem", padding: "6px 14px" }}
            onClick={() => setActiveTab("backup")}
          >
            <Save size={14} /> Backup & Restore
          </button>
          <button
            className={`btn ${activeTab === "integrity" ? "btn-primary" : "btn-secondary"}`}
            style={{ fontSize: "0.82rem", padding: "6px 14px" }}
            onClick={() => {
              setActiveTab("integrity");
              if (!integrityReport) {
                handleRunIntegrityCheck();
              }
            }}
          >
            <Activity size={14} /> Database Integrity
          </button>
        </div>

        <div className="modal-body settings-modal-body" style={{ maxHeight: "65vh", overflowY: "auto" }}>
          {/* TAB 1: Storage & Guarantees */}
          {activeTab === "storage" && (
            <>
              {/* Storage Locations */}
              <div className="settings-section">
                <div className="settings-section-title">
                  <Database size={15} color="#38bdf8" />
                  <span>Local Storage & Database</span>
                </div>
                <div className="settings-info-card">
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Local Database</span>
                    <span className="settings-kv-value" title={appInfo?.db_path ?? "Loading…"}>
                      {appInfo?.db_path ?? "Loading…"}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">App Data Directory</span>
                    <span className="settings-kv-value" title={appInfo?.app_data_dir ?? "Loading…"}>
                      {appInfo?.app_data_dir ?? "Loading…"}
                    </span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Operating System</span>
                    <span className="settings-kv-value">{appInfo?.os ?? "windows"}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">LibOne Version</span>
                    <span className="settings-kv-value">v{appInfo?.version ?? "0.1.0"}</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Database Schema Version</span>
                    <span className="settings-kv-value">v4 (Crash-Safe Recovery)</span>
                  </div>
                  <div className="settings-kv-row">
                    <span className="settings-kv-label">Export Format Spec</span>
                    <span className="settings-kv-value">v1.2.0</span>
                  </div>
                </div>
              </div>

              {/* Privacy & Safety Guarantees */}
              <div className="settings-section">
                <div className="settings-section-title">
                  <ShieldCheck size={15} color="#34d399" />
                  <span>Safety & Architecture Guarantees</span>
                </div>
                <div className="settings-guarantees-grid">
                  <div className="guarantee-item">
                    <div className="guarantee-icon-title">
                      <FolderLock size={16} color="#34d399" />
                      <strong>Originals Remain Untouched</strong>
                    </div>
                    <p>
                      LibOne accesses your folders in strict read-only mode. It never alters, moves, deletes, or writes to your original document files.
                    </p>
                  </div>

                  <div className="guarantee-item">
                    <div className="guarantee-icon-title">
                      <CloudOff size={16} color="#38bdf8" />
                      <strong>Zero Cloud Sync or Telemetry</strong>
                    </div>
                    <p>
                      No external synchronization, cloud uploads, or automated off-site backups. Your document library resides solely on your local hard drive.
                    </p>
                  </div>

                  <div className="guarantee-item">
                    <div className="guarantee-icon-title">
                      <FileCode size={16} color="#fbbf24" />
                      <strong>Metadata-Only Index</strong>
                    </div>
                    <p>
                      The SQLite database stores only filesystem metadata (path, size, timestamps, status). Full file contents are never copied into SQLite.
                    </p>
                  </div>

                  <div className="guarantee-item">
                    <div className="guarantee-icon-title">
                      <HardDrive size={16} color="#a78bfa" />
                      <strong>Atomic Portable Operations</strong>
                    </div>
                    <p>
                      Backups and exports use atomic write staging (.tmp + atomic rename) with validation to guarantee zero corruption or partial writes.
                    </p>
                  </div>
                </div>
              </div>
            </>
          )}

          {/* TAB 2: Backup & Restore */}
          {activeTab === "backup" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              {/* Backup Actions Card */}
              <div className="settings-section">
                <div className="settings-section-title">
                  <Save size={15} color="#38bdf8" />
                  <span>Local Metadata Backup</span>
                </div>
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 10 }}>
                  <p style={{ margin: 0, fontSize: "0.82rem", color: "var(--text-secondary)" }}>
                    Create a clean, atomic snapshot of your entire LibOne metadata database (roots, tags, collections, notes, and index records).
                    The backup is written locally using an atomic temporary sibling file.
                  </p>
                  <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 4 }}>
                    <button
                      className="btn btn-primary"
                      onClick={handleCreateBackup}
                      disabled={backupLoading}
                      style={{ display: "inline-flex", alignItems: "center", gap: 8 }}
                    >
                      <Save size={14} />
                      {backupLoading ? "Creating Backup…" : "Create Local Backup"}
                    </button>
                    <span style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
                      Never includes original file contents. Strictly local.
                    </span>
                  </div>

                  {backupSuccess && (
                    <div
                      style={{
                        marginTop: 8,
                        padding: "10px 12px",
                        background: "rgba(52, 211, 153, 0.1)",
                        border: "1px solid rgba(52, 211, 153, 0.3)",
                        borderRadius: "var(--radius-sm)",
                        fontSize: "0.8rem",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 6, color: "#34d399", fontWeight: 600 }}>
                        <CheckCircle2 size={16} /> Backup Created Successfully
                      </div>
                      <div style={{ marginTop: 4, fontFamily: "monospace", fontSize: "0.74rem", wordBreak: "break-all" }}>
                        Path: {backupSuccess.backup_path}
                      </div>
                      <div style={{ marginTop: 4, color: "var(--text-secondary)", fontSize: "0.75rem" }}>
                        Size: {(backupSuccess.file_size_bytes / 1024).toFixed(1)} KB | {backupSuccess.summary.total_files} indexed files | {backupSuccess.summary.total_collections} collections | {backupSuccess.summary.total_tags} tags
                      </div>
                    </div>
                  )}

                  {backupError && (
                    <div
                      style={{
                        marginTop: 8,
                        padding: "10px 12px",
                        background: "rgba(248, 113, 113, 0.1)",
                        border: "1px solid rgba(248, 113, 113, 0.3)",
                        borderRadius: "var(--radius-sm)",
                        fontSize: "0.8rem",
                        color: "#f87171",
                      }}
                    >
                      <AlertCircle size={16} style={{ display: "inline", verticalAlign: "text-bottom", marginRight: 6 }} />
                      Backup failed: {backupError}
                    </div>
                  )}
                </div>
              </div>

              {/* Restore Actions Card */}
              <div className="settings-section">
                <div className="settings-section-title">
                  <Upload size={15} color="#f59e0b" />
                  <span>Restore Library from Backup</span>
                </div>
                <div className="settings-info-card" style={{ flexDirection: "column", gap: 10 }}>
                  <p style={{ margin: 0, fontSize: "0.82rem", color: "var(--text-secondary)" }}>
                    Restore LibOne metadata from a previous local SQLite backup file. Before replacing your database,
                    the file is rigorously validated, and an automatic <strong>safety backup</strong> of your current database
                    is created.
                  </p>
                  <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 4 }}>
                    <button
                      className="btn btn-secondary"
                      onClick={handleSelectRestore}
                      disabled={restoreLoading || !!pendingRestoreValidation}
                      style={{ display: "inline-flex", alignItems: "center", gap: 8 }}
                    >
                      <Upload size={14} />
                      {restoreLoading ? "Validating Backup…" : "Select Backup File to Restore…"}
                    </button>
                  </div>

                  {/* Pending Confirmation Box */}
                  {pendingRestoreValidation && pendingRestorePath && (
                    <div
                      style={{
                        marginTop: 10,
                        padding: "12px 14px",
                        background: "rgba(245, 158, 11, 0.08)",
                        border: "1px solid rgba(245, 158, 11, 0.3)",
                        borderRadius: "var(--radius-sm)",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 8, color: "#f59e0b", fontWeight: 600, fontSize: "0.85rem" }}>
                        <AlertTriangle size={18} /> Confirm Metadata Restore
                      </div>
                      <div style={{ marginTop: 6, fontSize: "0.8rem", color: "var(--text-secondary)" }}>
                        Selected file: <span style={{ fontFamily: "monospace", color: "var(--text-primary)" }}>{pendingRestorePath}</span>
                      </div>

                      {pendingRestoreValidation.summary && (
                        <div style={{ margin: "8px 0", fontSize: "0.78rem", background: "var(--bg-panel)", padding: "8px 10px", borderRadius: 4 }}>
                          <div><strong>Backup Summary:</strong></div>
                          <div>Schema Version: v{pendingRestoreValidation.schema_version}</div>
                          <div>Indexed Files: {pendingRestoreValidation.summary.total_files}</div>
                          <div>Collections: {pendingRestoreValidation.summary.total_collections}</div>
                          <div>Tags: {pendingRestoreValidation.summary.total_tags}</div>
                          <div>Notes: {pendingRestoreValidation.summary.total_notes}</div>
                          <div>Scan Roots: {pendingRestoreValidation.summary.total_roots}</div>
                        </div>
                      )}

                      <div style={{ fontSize: "0.76rem", color: "var(--text-muted)", marginBottom: 10 }}>
                        <ShieldCheck size={14} style={{ display: "inline", verticalAlign: "middle", marginRight: 4, color: "#34d399" }} />
                        <strong>Safety Guarantee:</strong> An automatic pre-restore safety snapshot will be created in your app data folder.
                        Your actual files on disk will NOT be touched.
                      </div>

                      <div style={{ display: "flex", gap: 10 }}>
                        <button
                          className="btn btn-primary"
                          onClick={handleConfirmRestore}
                          disabled={restoreLoading}
                          style={{ fontSize: "0.8rem", background: "#f59e0b", borderColor: "#d97706" }}
                        >
                          {restoreLoading ? "Restoring Metadata…" : "Confirm & Replace Metadata"}
                        </button>
                        <button
                          className="btn btn-secondary"
                          onClick={() => {
                            setPendingRestoreValidation(null);
                            setPendingRestorePath(null);
                          }}
                          disabled={restoreLoading}
                          style={{ fontSize: "0.8rem" }}
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  )}

                  {restoreSuccess && (
                    <div
                      style={{
                        marginTop: 8,
                        padding: "10px 12px",
                        background: "rgba(52, 211, 153, 0.1)",
                        border: "1px solid rgba(52, 211, 153, 0.3)",
                        borderRadius: "var(--radius-sm)",
                        fontSize: "0.8rem",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 6, color: "#34d399", fontWeight: 600 }}>
                        <CheckCircle2 size={16} /> Library Restored Successfully
                      </div>
                      <div style={{ marginTop: 4, color: "var(--text-secondary)", fontSize: "0.75rem" }}>
                        Safety backup saved at: <span style={{ fontFamily: "monospace" }}>{restoreSuccess.safety_backup_path}</span>
                      </div>
                      <div style={{ marginTop: 2, color: "var(--text-secondary)", fontSize: "0.75rem" }}>
                        Restored {restoreSuccess.summary.total_files} files, {restoreSuccess.summary.total_collections} collections, and {restoreSuccess.summary.total_tags} tags.
                      </div>
                    </div>
                  )}

                  {restoreError && (
                    <div
                      style={{
                        marginTop: 8,
                        padding: "10px 12px",
                        background: "rgba(248, 113, 113, 0.1)",
                        border: "1px solid rgba(248, 113, 113, 0.3)",
                        borderRadius: "var(--radius-sm)",
                        fontSize: "0.8rem",
                        color: "#f87171",
                      }}
                    >
                      <AlertCircle size={16} style={{ display: "inline", verticalAlign: "text-bottom", marginRight: 6 }} />
                      {restoreError}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* TAB 3: Database Integrity & Diagnostics */}
          {activeTab === "integrity" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              <div className="settings-section">
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <div className="settings-section-title">
                    <Activity size={15} color="#38bdf8" />
                    <span>Metadata Integrity Diagnostics</span>
                  </div>
                  <button
                    className="btn btn-secondary"
                    onClick={handleRunIntegrityCheck}
                    disabled={integrityLoading}
                    style={{ fontSize: "0.76rem", padding: "4px 10px" }}
                  >
                    <RefreshCw size={12} className={integrityLoading ? "spin" : ""} />
                    {integrityLoading ? "Checking…" : "Re-run Diagnostic"}
                  </button>
                </div>

                <div className="settings-info-card" style={{ flexDirection: "column", gap: 12 }}>
                  <p style={{ margin: 0, fontSize: "0.82rem", color: "var(--text-secondary)" }}>
                    Verifies database consistency, required schema tables, foreign key constraints, and validates
                    registered file path accessibility without modifying or silently deleting any records.
                  </p>

                  {integrityError && (
                    <div
                      style={{
                        padding: "10px 12px",
                        background: "rgba(248, 113, 113, 0.1)",
                        border: "1px solid rgba(248, 113, 113, 0.3)",
                        borderRadius: "var(--radius-sm)",
                        fontSize: "0.8rem",
                        color: "#f87171",
                      }}
                    >
                      <AlertCircle size={16} style={{ display: "inline", verticalAlign: "text-bottom", marginRight: 6 }} />
                      Diagnostic error: {integrityError}
                    </div>
                  )}

                  {integrityReport && (
                    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                      {/* Overall Status Banner */}
                      <div
                        style={{
                          padding: "10px 14px",
                          borderRadius: "var(--radius-sm)",
                          background: integrityReport.is_healthy
                            ? "rgba(52, 211, 153, 0.1)"
                            : "rgba(245, 158, 11, 0.1)",
                          border: `1px solid ${
                            integrityReport.is_healthy
                              ? "rgba(52, 211, 153, 0.3)"
                              : "rgba(245, 158, 11, 0.3)"
                          }`,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "space-between",
                        }}
                      >
                        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                          {integrityReport.is_healthy ? (
                            <CheckCircle2 size={18} color="#34d399" />
                          ) : (
                            <AlertTriangle size={18} color="#f59e0b" />
                          )}
                          <div>
                            <strong style={{ fontSize: "0.85rem", color: integrityReport.is_healthy ? "#34d399" : "#f59e0b" }}>
                              {integrityReport.is_healthy ? "Database is Healthy" : "Integrity Issues Detected"}
                            </strong>
                            <div style={{ fontSize: "0.74rem", color: "var(--text-secondary)" }}>
                              SQLite quick_check: {integrityReport.integrity_check_passed ? "Passed (ok)" : "Issues detected"} | Schema Version: v{integrityReport.schema_version}
                            </div>
                          </div>
                        </div>
                      </div>

                      {/* Detailed Diagnostic Items */}
                      <div className="settings-kv-row">
                        <span className="settings-kv-label">Foreign Key Violations</span>
                        <span className="settings-kv-value" style={{ color: integrityReport.foreign_key_violations.length > 0 ? "#f87171" : "#34d399" }}>
                          {integrityReport.foreign_key_violations.length === 0 ? "0 (None)" : `${integrityReport.foreign_key_violations.length} violations`}
                        </span>
                      </div>
                      <div className="settings-kv-row">
                        <span className="settings-kv-label">Missing Registered Files on Disk</span>
                        <span className="settings-kv-value" style={{ color: integrityReport.missing_files_count > 0 ? "#f59e0b" : "#34d399" }}>
                          {integrityReport.missing_files_count} file(s)
                        </span>
                      </div>
                      <div className="settings-kv-row">
                        <span className="settings-kv-label">Invalid File Paths</span>
                        <span className="settings-kv-value" style={{ color: integrityReport.invalid_paths_count > 0 ? "#f87171" : "#34d399" }}>
                          {integrityReport.invalid_paths_count} path(s)
                        </span>
                      </div>
                      <div className="settings-kv-row">
                        <span className="settings-kv-label">Total Issues Logged</span>
                        <span className="settings-kv-value">
                          {integrityReport.issues.length}
                        </span>
                      </div>

                      {/* Issues List & Repair Guidance */}
                      {integrityReport.issues.length > 0 && (
                        <div style={{ marginTop: 6, display: "flex", flexDirection: "column", gap: 8 }}>
                          <span style={{ fontSize: "0.8rem", fontWeight: 600 }}>Actionable Repair Guidance:</span>
                          {integrityReport.issues.map((issue, idx) => (
                            <div
                              key={idx}
                              style={{
                                padding: "8px 10px",
                                background: "var(--bg-panel)",
                                borderLeft: `3px solid ${
                                  issue.severity === "error"
                                    ? "#f87171"
                                    : issue.severity === "warning"
                                    ? "#f59e0b"
                                    : "#38bdf8"
                                }`,
                                borderRadius: "0 4px 4px 0",
                                fontSize: "0.78rem",
                              }}
                            >
                              <div style={{ fontWeight: 600, color: "var(--text-primary)", marginBottom: 2 }}>
                                [{issue.category}] {issue.message}
                              </div>
                              <div style={{ color: "var(--text-secondary)", fontSize: "0.75rem", display: "flex", alignItems: "flex-start", gap: 4 }}>
                                <HelpCircle size={13} style={{ flexShrink: 0, marginTop: 2, color: "#38bdf8" }} />
                                <span>{issue.repair_guidance}</span>
                              </div>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>

        <div className="modal-footer" style={{ justifyContent: "space-between" }}>
          <button
            className="btn btn-secondary"
            onClick={() => {
              onClose();
              onOpenExport();
            }}
            title="Export full library metadata to portable JSON"
          >
            <Download size={14} /> Open Metadata Export
          </button>
          <button className="btn btn-primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
};
