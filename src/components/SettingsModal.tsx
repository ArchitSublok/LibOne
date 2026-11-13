import React from "react";
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
} from "lucide-react";
import { AppInfo } from "../types";

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  appInfo: AppInfo | null;
  onOpenExport: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  appInfo,
  onOpenExport,
}) => {
  if (!isOpen) return null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal-content settings-modal-content"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Settings size={20} color="#38bdf8" />
            <h3 style={{ fontSize: "1.1rem" }}>Settings & Library Storage</h3>
          </div>
          <button className="btn btn-danger-ghost" onClick={onClose}>
            <X size={18} />
          </button>
        </div>

        <div className="modal-body settings-modal-body">
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
                <span className="settings-kv-value">
                  {appInfo?.os ?? "windows"}
                </span>
              </div>
              <div className="settings-kv-row">
                <span className="settings-kv-label">LibOne Version</span>
                <span className="settings-kv-value">
                  v{appInfo?.version ?? "0.1.0"}
                </span>
              </div>
              <div className="settings-kv-row">
                <span className="settings-kv-label">Export Format Spec</span>
                <span className="settings-kv-value">v1.0.0</span>
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
                  <strong>Atomic Portable Export</strong>
                </div>
                <p>
                  Exports use a safe-write mechanism (.tmp + atomic rename) and schema validation, producing clean, portable JSON compatible with standard tools.
                </p>
              </div>
            </div>
          </div>
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
