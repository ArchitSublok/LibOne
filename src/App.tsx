import React, { useState, useEffect } from "react";
import {
  RefreshCw,
  Download,
  ExternalLink,
  FileText,
  Search,
  CheckCircle2,
  AlertTriangle,
  MoveRight,
  Copy,
  Check,
  X,
  Scan,
  FolderOpen,
  Info,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ArrowUpDown,
  ArrowUp,
  ArrowDown,
  Activity,
  Settings,
  FolderPlus,
  FolderMinus,
  Tag as TagIcon,
  Tags,
  SquareCheck,
  Sparkles,
} from "lucide-react";
import {
  AppInfo,
  Collection,
  FileDetail,
  IndexedFile,
  LibraryExport,
  PaginatedFiles,
  ScanRoot,
  Tag,
  WatcherOverallStatus,
} from "./types";
import {
  addFilesToCollection,
  addFilesToTag,
  deleteCollection,
  deleteTag,
  exportLibraryMetadata,
  getAppInfo,
  getCollectionCounts,
  getIndexedFileDetail,
  getLibraryExtensions,
  getTagCounts,
  getWatcherStatus,
  listenLibraryUpdated,
  listenWatcherStatus,
  listCollections,
  listPendingSuggestions,
  listScanRoots,
  listTags,
  openOriginalFile,
  promptSaveExportPath,
  queryIndexedFiles,
  removeFilesFromCollection,
  removeFilesFromTag,
  scanAllRoots,
  showInFileExplorer,
} from "./services/api";
import { ScanRootsPanel } from "./components/ScanRootsPanel";
import { ScanScreen } from "./components/ScanScreen";
import { FileDetailModal } from "./components/FileDetailModal";
import { SettingsModal } from "./components/SettingsModal";
import { DiagnosticsModal } from "./components/DiagnosticsModal";
import { CollectionModal } from "./components/CollectionModal";
import { TagModal } from "./components/TagModal";
import { OrganizationPanel } from "./components/OrganizationPanel";
import { BulkAssignModal } from "./components/BulkAssignModal";
import { ConfirmDeleteDialog } from "./components/ConfirmDeleteDialog";
import { SuggestionsView } from "./components/SuggestionsView";
import { useDebouncedValue } from "./hooks/useDebouncedValue";
import { useResetPageOnFilters } from "./hooks/useResetPageOnFilters";
import "./App.css";

export const App: React.FC = () => {
  // ── Core state ────────────────────────────────────────────────────────────
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [roots, setRoots] = useState<ScanRoot[]>([]);
  const [rootsLoading, setRootsLoading] = useState(true);
  const [rootsError, setRootsError] = useState<string | null>(null);

  // ── File query & pagination state ─────────────────────────────────────────
  const [selectedRootId, setSelectedRootId] = useState<number | null>(null);
  const [files, setFiles] = useState<IndexedFile[]>([]);
  const [filesLoading, setFilesLoading] = useState(false);
  const [totalFiles, setTotalFiles] = useState(0);
  const [totalPages, setTotalPages] = useState(1);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(25);

  // ── Sorting & filtering state ─────────────────────────────────────────────
  // The text box is debounced; all explicit filters below apply immediately.
  const [searchInput, setSearchInput] = useState("");
  const searchQuery = useDebouncedValue(searchInput, 300);
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const [extensionFilter, setExtensionFilter] = useState<string>("all");
  const [distinctExtensions, setDistinctExtensions] = useState<string[]>([]);
  const [sortBy, setSortBy] = useState<
    "name" | "path" | "extension" | "size" | "modified" | "status"
  >("name");
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("asc");

  // ── Phase 2 organization state (collections & tags) ───────────────────────
  const [collections, setCollections] = useState<Collection[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [collectionCounts, setCollectionCounts] = useState<Record<number, number>>({});
  const [tagCounts, setTagCounts] = useState<Record<number, number>>({});
  const [organizationLoading, setOrganizationLoading] = useState(true);
  const [organizationError, setOrganizationError] = useState<string | null>(null);

  const [selectedCollectionId, setSelectedCollectionId] = useState<number | null>(null);
  const [selectedTagIds, setSelectedTagIds] = useState<number[]>([]);

  const [collectionModalOpen, setCollectionModalOpen] = useState(false);
  const [editingCollection, setEditingCollection] = useState<Collection | null>(null);
  const [tagModalOpen, setTagModalOpen] = useState(false);
  const [editingTag, setEditingTag] = useState<Tag | null>(null);
  const [pendingDelete, setPendingDelete] = useState<{
    kind: "collection" | "tag";
    id: number;
    name: string;
  } | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  // ── Multi-select & bulk organization state ────────────────────────────────
  const [selectedFileIds, setSelectedFileIds] = useState<number[]>([]);
  const [bulkAction, setBulkAction] = useState<{
    kind: "collection" | "tag";
    action: "add" | "remove";
  } | null>(null);

  const handleLibraryRestored = () => {
    loadRoots();
    loadExtensions();
    loadOrganization();
    loadFiles();
    refreshPendingSuggestionCount();
  };

  // ── Detail view & scanner states ──────────────────────────────────────────
  const [selectedDetail, setSelectedDetail] = useState<FileDetail | null>(null);
  const [detailModalOpen, setDetailModalOpen] = useState(false);
  const [actionAlert, setActionAlert] = useState<{
    type: "error" | "warning" | "success";
    message: string;
  } | null>(null);

  const [isScanning, setIsScanning] = useState(false);
  const [scanBanner, setScanBanner] = useState<{
    message: string;
    durationMs?: number;
  } | null>(null);
  const [scanScreenOpen, setScanScreenOpen] = useState(false);
  const [scanScreenRootId, setScanScreenRootId] = useState<number | null>(null);

  // ── Watcher state ─────────────────────────────────────────────────────────
  const [watcherStatus, setWatcherStatus] = useState<WatcherOverallStatus | null>(null);
  const [showWatcherPopover, setShowWatcherPopover] = useState(false);

  // ── Settings modal ────────────────────────────────────────────────────────
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);

  // ── Export modal ──────────────────────────────────────────────────────────
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportData, setExportData] = useState<LibraryExport | null>(null);
  const [copiedExport, setCopiedExport] = useState(false);
  const [exportStatusText, setExportStatusText] = useState<string | null>(null);

  // ── Phase 3 suggestions state ─────────────────────────────────────────────
  const [currentView, setCurrentView] = useState<"library" | "suggestions">("library");
  const [pendingSuggestionCount, setPendingSuggestionCount] = useState<number>(0);

  const refreshPendingSuggestionCount = async () => {
    try {
      const pending = await listPendingSuggestions();
      setPendingSuggestionCount(pending.length);
    } catch {
      // silent fallback
    }
  };

  // ── Bootstrap ─────────────────────────────────────────────────────────────
  useEffect(() => {
    getAppInfo()
      .then(setAppInfo)
      .catch((e) => console.error("Failed to load app info", e));
    loadRoots();
    loadExtensions();
    loadOrganization();
    refreshPendingSuggestionCount();

    // Initial watcher status
    getWatcherStatus()
      .then(setWatcherStatus)
      .catch((e) => console.warn("Failed to get watcher status", e));

    let unlistenStatus: (() => void) | undefined;
    let unlistenUpdated: (() => void) | undefined;

    listenWatcherStatus((status) => {
      setWatcherStatus(status);
    }).then((fn) => {
      unlistenStatus = fn;
    });

    listenLibraryUpdated(() => {
      // Background watcher sync completed: refresh file list & extensions silently
      loadExtensions();
      loadFiles();
      loadRoots();
      refreshPendingSuggestionCount();
    }).then((fn) => {
      unlistenUpdated = fn;
    });

    return () => {
      if (unlistenStatus) unlistenStatus();
      if (unlistenUpdated) unlistenUpdated();
    };
  }, []);

  // Pagination resets on any explicit filter/sort change and when the debounced
  // search query settles. Typing alone does not fire extra queries.
  const filterKey = JSON.stringify([
    selectedRootId,
    searchQuery,
    statusFilter,
    extensionFilter,
    selectedCollectionId,
    selectedTagIds,
    sortBy,
    sortOrder,
  ]);
  useResetPageOnFilters(filterKey, setPage);

  // Query files when page or filters change
  useEffect(() => {
    loadFiles();
  }, [
    page,
    pageSize,
    selectedRootId,
    searchQuery,
    statusFilter,
    extensionFilter,
    selectedCollectionId,
    selectedTagIds,
    sortBy,
    sortOrder,
  ]);

  // A selection only applies to the currently visible page/filter set.
  useEffect(() => {
    setSelectedFileIds([]);
  }, [
    page,
    pageSize,
    selectedRootId,
    searchQuery,
    statusFilter,
    extensionFilter,
    selectedCollectionId,
    selectedTagIds,
    sortBy,
    sortOrder,
  ]);

  // ── Data loaders ──────────────────────────────────────────────────────────

  const loadRoots = async () => {
    setRootsLoading(true);
    setRootsError(null);
    try {
      const list = await listScanRoots();
      setRoots(list);
      getWatcherStatus().then(setWatcherStatus).catch(() => {});
    } catch (e: any) {
      setRootsError(e?.message ?? String(e));
    } finally {
      setRootsLoading(false);
    }
  };

  const loadExtensions = async () => {
    try {
      const exts = await getLibraryExtensions();
      setDistinctExtensions(exts);
    } catch (e) {
      console.warn("Failed to load distinct extensions", e);
    }
  };

  const loadOrganization = async () => {
    setOrganizationLoading(true);
    setOrganizationError(null);
    try {
      const [collectionList, tagList] = await Promise.all([
        listCollections(),
        listTags(),
      ]);
      setCollections(collectionList);
      setTags(tagList);

      const [collectionCountMap, tagCountMap] = await Promise.all([
        getCollectionCounts(collectionList),
        getTagCounts(tagList),
      ]);
      setCollectionCounts(collectionCountMap);
      setTagCounts(tagCountMap);
    } catch (e: any) {
      setOrganizationError(e?.message ?? String(e));
    } finally {
      setOrganizationLoading(false);
    }
  };

  const loadFiles = async () => {
    setFilesLoading(true);
    try {
      const paginated: PaginatedFiles = await queryIndexedFiles({
        query: searchQuery || null,
        root_id: selectedRootId,
        extension: extensionFilter !== "all" ? extensionFilter : null,
        status: statusFilter !== "all" ? statusFilter : null,
        collection_id: selectedCollectionId,
        tag_ids: selectedTagIds.length > 0 ? selectedTagIds : null,
        sort_by: sortBy,
        sort_order: sortOrder,
        page,
        page_size: pageSize,
      });

      setFiles(paginated.files);
      setTotalFiles(paginated.total);
      setTotalPages(paginated.total_pages);
    } catch (e) {
      console.error("Failed to query files", e);
    } finally {
      setFilesLoading(false);
    }
  };

  // ── Sorting handler ───────────────────────────────────────────────────────

  const handleSort = (
    column: "name" | "path" | "extension" | "size" | "modified" | "status"
  ) => {
    if (sortBy === column) {
      setSortOrder(sortOrder === "asc" ? "desc" : "asc");
    } else {
      setSortBy(column);
      setSortOrder("asc");
    }
  };

  const renderSortIndicator = (
    column: "name" | "path" | "extension" | "size" | "modified" | "status"
  ) => {
    if (sortBy !== column) {
      return <ArrowUpDown size={12} className="sort-indicator-inactive" />;
    }
    return sortOrder === "asc" ? (
      <ArrowUp size={13} className="sort-indicator" />
    ) : (
      <ArrowDown size={13} className="sort-indicator" />
    );
  };

  // ── File action handlers ──────────────────────────────────────────────────

  const handleOpenFile = async (fileId: number, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    setActionAlert(null);
    try {
      await openOriginalFile(fileId);
    } catch (err: any) {
      const msg = err?.message || String(err);
      setActionAlert({ type: "error", message: msg });
      loadFiles(); // refresh to reflect 'missing' status if file disappeared
    }
  };

  const handleShowInExplorer = async (fileId: number, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    setActionAlert(null);
    try {
      await showInFileExplorer(fileId);
    } catch (err: any) {
      const msg = err?.message || String(err);
      setActionAlert({ type: "warning", message: msg });
      loadFiles();
    }
  };

  const handleViewDetail = async (fileId: number, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    setActionAlert(null);
    try {
      const detail = await getIndexedFileDetail(fileId);
      setSelectedDetail(detail);
      setDetailModalOpen(true);
    } catch (err: any) {
      setActionAlert({ type: "error", message: err?.message || String(err) });
    }
  };

  // ── Collection & tag filter handlers ──────────────────────────────────────

  const handleSelectCollection = (id: number | null) => {
    setSelectedCollectionId(id);
  };

  const handleToggleTag = (tagId: number) => {
    setSelectedTagIds((prev) =>
      prev.includes(tagId) ? prev.filter((id) => id !== tagId) : [...prev, tagId]
    );
  };

  const handleClearOrganizationFilters = () => {
    setSelectedCollectionId(null);
    setSelectedTagIds([]);
  };

  // ── Collection management ─────────────────────────────────────────────────

  const openCreateCollection = () => {
    setEditingCollection(null);
    setCollectionModalOpen(true);
  };

  const openEditCollection = (collection: Collection) => {
    setEditingCollection(collection);
    setCollectionModalOpen(true);
  };

  const closeCollectionModal = () => {
    setCollectionModalOpen(false);
    setEditingCollection(null);
  };

  const handleCollectionSaved = async (saved: Collection) => {
    const wasEditing = editingCollection !== null;
    await loadOrganization();
    setActionAlert({
      type: "success",
      message: wasEditing
        ? `Collection "${saved.name}" was updated in LibOne. Original files were not modified.`
        : `Collection "${saved.name}" was created in LibOne. Original files were not modified.`,
    });
  };

  // ── Tag management ────────────────────────────────────────────────────────

  const openCreateTag = () => {
    setEditingTag(null);
    setTagModalOpen(true);
  };

  const openEditTag = (tag: Tag) => {
    setEditingTag(tag);
    setTagModalOpen(true);
  };

  const closeTagModal = () => {
    setTagModalOpen(false);
    setEditingTag(null);
  };

  const handleTagSaved = async (saved: Tag) => {
    const wasEditing = editingTag !== null;
    await loadOrganization();
    setActionAlert({
      type: "success",
      message: wasEditing
        ? `Tag "${saved.name}" was updated in LibOne. Original files were not modified.`
        : `Tag "${saved.name}" was created in LibOne. Original files were not modified.`,
    });
  };

  // ── Delete collection / tag (metadata only) ───────────────────────────────

  const requestDeleteCollection = (collection: Collection) => {
    setDeleteError(null);
    setPendingDelete({
      kind: "collection",
      id: collection.id,
      name: collection.name,
    });
  };

  const requestDeleteTag = (tag: Tag) => {
    setDeleteError(null);
    setPendingDelete({ kind: "tag", id: tag.id, name: tag.name });
  };

  const closeDeleteDialog = () => {
    if (deleteBusy) return;
    setPendingDelete(null);
    setDeleteError(null);
  };

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return;
    const target = pendingDelete;
    setDeleteBusy(true);
    setDeleteError(null);
    try {
      if (target.kind === "collection") {
        await deleteCollection(target.id);
        if (selectedCollectionId === target.id) {
          setSelectedCollectionId(null);
        }
      } else {
        await deleteTag(target.id);
        setSelectedTagIds((prev) =>
          prev.includes(target.id) ? prev.filter((id) => id !== target.id) : prev
        );
      }

      const { kind, name } = target;
      setPendingDelete(null);
      await loadOrganization();
      setActionAlert({
        type: "success",
        message: `${
          kind === "collection" ? "Collection" : "Tag"
        } "${name}" was deleted from LibOne. Original files on disk were not changed.`,
      });
    } catch (e: any) {
      setDeleteError(e?.message ?? String(e));
    } finally {
      setDeleteBusy(false);
    }
  };

  // ── Multi-file selection & organization actions ───────────────────────────

  const toggleFileSelection = (fileId: number) => {
    setSelectedFileIds((prev) =>
      prev.includes(fileId) ? prev.filter((id) => id !== fileId) : [...prev, fileId]
    );
  };

  const togglePageSelection = () => {
    const pageIds = files.map((file) => file.id);
    setSelectedFileIds((prev) =>
      pageIds.length > 0 && pageIds.every((id) => prev.includes(id)) ? [] : pageIds
    );
  };

  const openBulkAction = (
    kind: "collection" | "tag",
    action: "add" | "remove"
  ) => {
    setBulkAction({ kind, action });
  };

  const handleBulkConfirm = async (targetId: number) => {
    if (!bulkAction) return;
    const fileIds = [...selectedFileIds];
    if (fileIds.length === 0) {
      throw new Error("No files are selected.");
    }

    const { kind, action } = bulkAction;
    const targetName =
      kind === "collection"
        ? collections.find((c) => c.id === targetId)?.name ?? "collection"
        : tags.find((t) => t.id === targetId)?.name ?? "tag";

    if (kind === "collection") {
      if (action === "add") {
        await addFilesToCollection(fileIds, targetId);
      } else {
        await removeFilesFromCollection(fileIds, targetId);
      }
    } else {
      if (action === "add") {
        await addFilesToTag(fileIds, targetId);
      } else {
        await removeFilesFromTag(fileIds, targetId);
      }
    }

    await loadOrganization();
    await loadFiles();
    setSelectedFileIds([]);
    setActionAlert({
      type: "success",
      message: `${
        action === "add" ? "Added" : "Removed"
      } ${fileIds.length} file${fileIds.length === 1 ? "" : "s"} ${
        action === "add" ? "to" : "from"
      } ${kind} "${targetName}" in LibOne. Original files were not modified.`,
    });
  };

  // ── Scan all ──────────────────────────────────────────────────────────────

  const handleScanAll = async () => {
    const enabled = roots.filter((r) => r.enabled);
    if (enabled.length === 0) {
      setScanBanner({ message: "No enabled folders to scan. Add a folder first." });
      return;
    }
    setIsScanning(true);
    setScanBanner({ message: "Scanning all enabled folders…" });
    try {
      const statsList = await scanAllRoots();
      const totalScanned = statsList.reduce((a, s) => a + s.total_scanned, 0);
      const totalAdded = statsList.reduce((a, s) => a + s.added, 0);
      const totalMod = statsList.reduce((a, s) => a + s.modified, 0);
      const totalMoved = statsList.reduce((a, s) => a + s.moved, 0);
      const totalMissing = statsList.reduce((a, s) => a + s.missing, 0);
      const totalMs = statsList.reduce((a, s) => a + s.duration_ms, 0);
      setScanBanner({
        message: `Scan complete — ${totalScanned} files (${totalAdded} added, ${totalMod} modified, ${totalMoved} moved, ${totalMissing} missing)`,
        durationMs: totalMs,
      });
      await loadRoots();
      await loadExtensions();
      await loadFiles();
    } catch (e: any) {
      setScanBanner({ message: `Scan all failed: ${e?.message ?? e}` });
    } finally {
      setIsScanning(false);
    }
  };

  // ── Export ────────────────────────────────────────────────────────────────

  const handleOpenExportModal = async () => {
    try {
      const data = await exportLibraryMetadata();
      setExportData(data);
      setCopiedExport(false);
      setExportStatusText(null);
      setExportModalOpen(true);
    } catch (e: any) {
      alert(`Export error: ${e?.message ?? e}`);
    }
  };

  const handleSaveExportToFile = async () => {
    try {
      const savePath = await promptSaveExportPath();
      if (!savePath) return;
      await exportLibraryMetadata(savePath);
      setExportStatusText(`Saved to: ${savePath}`);
    } catch (e: any) {
      alert(`Failed to save export file: ${e?.message ?? e}`);
    }
  };

  const handleCopyExportJson = () => {
    if (!exportData) return;
    navigator.clipboard.writeText(JSON.stringify(exportData, null, 2));
    setCopiedExport(true);
    setTimeout(() => setCopiedExport(false), 2500);
  };

  // ── Formatting helpers ────────────────────────────────────────────────────

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

  const getExtensionClass = (ext: string): string => {
    const e = ext.toLowerCase();
    if (e === "pdf") return "ext-pdf";
    if (e === "docx" || e === "doc") return "ext-docx";
    if (e === "xlsx" || e === "xls" || e === "csv") return "ext-xlsx";
    if (e === "txt" || e === "md") return "ext-txt";
    return "";
  };

  // ── Derived view state ────────────────────────────────────────────────────

  const activeCollection =
    selectedCollectionId !== null
      ? collections.find((c) => c.id === selectedCollectionId) ?? null
      : null;
  const activeTags = selectedTagIds
    .map((id) => tags.find((t) => t.id === id))
    .filter((t): t is Tag => Boolean(t));
  const hasOrganizationFilter =
    selectedCollectionId !== null || selectedTagIds.length > 0;
  const hasAnyFilter =
    Boolean(searchQuery) ||
    statusFilter !== "all" ||
    extensionFilter !== "all" ||
    hasOrganizationFilter;

  const allPageSelected =
    files.length > 0 && files.every((file) => selectedFileIds.includes(file.id));
  const somePageSelected = files.some((file) => selectedFileIds.includes(file.id));

  const bulkOptions = bulkAction
    ? bulkAction.kind === "collection"
      ? collections.map((collection) => ({
          id: collection.id,
          name: collection.name,
          color: collection.color_nullable,
          count: collectionCounts[collection.id] ?? 0,
        }))
      : tags.map((tag) => ({
          id: tag.id,
          name: tag.name,
          color: tag.color_nullable,
          count: tagCounts[tag.id] ?? 0,
        }))
    : [];

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="app-container">
      {/* ── Header ───────────────────────────────────────────────────────── */}
      <header className="app-header">
        <div className="brand-section">
          <div className="logo-badge">L1</div>
          <div>
            <div className="brand-title">
              LibOne{" "}
              {appInfo?.version && (
                <span style={{ fontSize: "0.7rem", opacity: 0.6 }}>
                  v{appInfo.version}
                </span>
              )}
            </div>
            <div className="brand-subtitle">
              Local-first document library for Windows
            </div>
          </div>
        </div>

        {/* ── Live File-System Watcher Status Pill ────────────────────────── */}
        <div className="watcher-indicator-container">
          <button
            type="button"
            className={`watcher-indicator-pill ${
              watcherStatus?.is_refreshing || watcherStatus?.is_coalescing
                ? "syncing"
                : watcherStatus?.root_statuses.some((r) => r.error_message)
                ? "error"
                : watcherStatus?.is_watching && (watcherStatus.watched_roots_count ?? 0) > 0
                ? "live"
                : "idle"
            }`}
            onClick={() => setShowWatcherPopover(!showWatcherPopover)}
            title="Click to view file-system watcher status"
          >
            {watcherStatus?.is_refreshing || watcherStatus?.is_coalescing ? (
              <>
                <RefreshCw size={12} className="spin" />
                <span>Syncing changes…</span>
              </>
            ) : watcherStatus?.root_statuses.some((r) => r.error_message) ? (
              <>
                <AlertTriangle size={12} className="text-warning" />
                <span>Watcher alert</span>
              </>
            ) : watcherStatus?.is_watching && (watcherStatus.watched_roots_count ?? 0) > 0 ? (
              <>
                <span className="live-dot" />
                <span>
                  Watching {watcherStatus.watched_roots_count} folder
                  {watcherStatus.watched_roots_count === 1 ? "" : "s"}
                </span>
              </>
            ) : (
              <>
                <span className="idle-dot" />
                <span>Watcher idle</span>
              </>
            )}
          </button>

          {showWatcherPopover && (
            <div className="watcher-popover">
              <div className="watcher-popover-header">
                <div className="watcher-popover-title">
                  <Activity size={14} /> File-System Watcher
                </div>
                <button
                  type="button"
                  className="btn-icon"
                  onClick={() => setShowWatcherPopover(false)}
                >
                  <X size={14} />
                </button>
              </div>
              <div className="watcher-popover-body">
                <div className="watcher-stat-row">
                  <span className="watcher-stat-label">Status</span>
                  <span className="watcher-stat-val">
                    {watcherStatus?.is_refreshing
                      ? "Refreshing library…"
                      : watcherStatus?.is_coalescing
                      ? "Coalescing file events…"
                      : watcherStatus?.is_watching && (watcherStatus.watched_roots_count ?? 0) > 0
                      ? "Monitoring for changes"
                      : "Inactive"}
                  </span>
                </div>
                <div className="watcher-stat-row">
                  <span className="watcher-stat-label">Last sync</span>
                  <span className="watcher-stat-val">
                    {watcherStatus?.last_refreshed_at
                      ? formatDate(watcherStatus.last_refreshed_at)
                      : "None yet"}
                  </span>
                </div>

                <div className="watcher-roots-list-header">Watched Roots</div>
                <div className="watcher-roots-list">
                  {!watcherStatus?.root_statuses || watcherStatus.root_statuses.length === 0 ? (
                    <div className="watcher-root-empty">No roots currently watched</div>
                  ) : (
                    watcherStatus.root_statuses.map((r) => (
                      <div key={r.root_id} className="watcher-root-item">
                        <div className="watcher-root-path" title={r.path}>
                          {r.path}
                        </div>
                        <div className="watcher-root-badges">
                          {r.error_message ? (
                            <span className="badge badge-error" title={r.error_message}>
                              Error
                            </span>
                          ) : r.active ? (
                            <span className="badge badge-success">Active</span>
                          ) : (
                            <span className="badge badge-secondary">Paused</span>
                          )}
                        </div>
                        {r.error_message && (
                          <div className="watcher-root-error">{r.error_message}</div>
                        )}
                      </div>
                    ))
                  )}
                </div>
                <div className="watcher-popover-footer">
                  Debounces bursts (500ms window). Excludes symlinks, junctions, and build artifacts.
                </div>
              </div>
            </div>
          )}
        </div>

        <div className="header-actions">
          <button
            className={`btn ${currentView === "suggestions" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => setCurrentView(currentView === "suggestions" ? "library" : "suggestions")}
            title="Review local organization suggestions"
          >
            <Sparkles size={14} /> Suggestions
            {pendingSuggestionCount > 0 && (
              <span
                style={{
                  marginLeft: 6,
                  padding: "1px 6px",
                  borderRadius: "9999px",
                  background: currentView === "suggestions" ? "rgba(255, 255, 255, 0.25)" : "var(--primary-color, #2563eb)",
                  color: "#fff",
                  fontSize: "0.72rem",
                  fontWeight: 600,
                }}
              >
                {pendingSuggestionCount}
              </span>
            )}
          </button>

          <button
            className="btn btn-secondary"
            onClick={() => {
              setScanScreenRootId(selectedRootId ?? (roots[0]?.id ?? null));
              setScanScreenOpen(true);
            }}
            disabled={isScanning || roots.length === 0}
            title="Open real-time document scanner"
          >
            <Scan size={14} /> Scan Screen
          </button>

          <button
            className="btn btn-secondary"
            onClick={handleScanAll}
            disabled={isScanning || roots.length === 0}
            title="Incrementally scan all enabled folders"
          >
            <RefreshCw size={14} className={isScanning ? "spin" : ""} />
            {isScanning ? "Scanning…" : "Scan All"}
          </button>

          <button
            className="btn btn-secondary"
            onClick={handleOpenExportModal}
            title="Export library metadata to JSON"
          >
            <Download size={14} /> Export
          </button>

          <button
            className="btn btn-secondary"
            onClick={() => setDiagnosticsOpen(true)}
            title="View privacy-safe diagnostics, health status, and system metrics"
          >
            <Activity size={14} /> Diagnostics
          </button>

          <button
            className="btn btn-secondary"
            onClick={() => setSettingsOpen(true)}
            title="View database location, storage, and safety guarantees"
          >
            <Settings size={14} /> Settings
          </button>
        </div>
      </header>

      {/* ── Body grid or Scan Screen or Suggestions View ─────────────────────── */}
      {scanScreenOpen ? (
        <ScanScreen
          roots={roots}
          initialRootId={scanScreenRootId}
          onClose={() => {
            setScanScreenOpen(false);
            loadRoots();
            loadExtensions();
            loadFiles();
            refreshPendingSuggestionCount();
          }}
          onScanCompleted={() => {
            loadRoots();
            loadExtensions();
            loadFiles();
            refreshPendingSuggestionCount();
          }}
        />
      ) : currentView === "suggestions" ? (
        <SuggestionsView
          onBackToLibrary={() => {
            setCurrentView("library");
            loadFiles();
            loadRoots();
            loadOrganization();
            refreshPendingSuggestionCount();
          }}
          onSuggestionsChanged={() => {
            refreshPendingSuggestionCount();
            loadOrganization();
          }}
        />
      ) : (
        <div className="main-layout">
          {/* ── Sidebar: Scan Roots Panel ───────────────────────────────────── */}
          {rootsLoading ? (
            <aside className="sidebar">
              <div className="loading-state" style={{ padding: 24 }}>
                <RefreshCw size={18} className="spin" />
                <p style={{ marginTop: 8, fontSize: "0.82rem" }}>Loading folders…</p>
              </div>
            </aside>
          ) : rootsError ? (
            <aside className="sidebar">
              <div className="inline-error" style={{ margin: 20 }}>
                <AlertTriangle size={15} />
                <span>{rootsError}</span>
              </div>
            </aside>
          ) : (
            <ScanRootsPanel
              roots={roots}
              selectedRootId={selectedRootId}
              totalFileCount={totalFiles}
              appDataDbPath={appInfo?.db_path}
              isScanning={isScanning}
              onSelectRoot={(id) => setSelectedRootId(id)}
              onRootsChanged={async () => {
                await loadRoots();
                await loadExtensions();
                await loadFiles();
              }}
              onScanComplete={(stats) => {
                setScanBanner({
                  message: `Scan complete — ${stats.total_scanned} files (${stats.added} added, ${stats.modified} modified, ${stats.moved} moved, ${stats.missing} missing)`,
                  durationMs: stats.duration_ms,
                });
                loadRoots();
                loadExtensions();
                loadFiles();
              }}
              onScanningChange={setIsScanning}
              onOpenScanScreen={(id) => {
                setScanScreenRootId(id);
                setScanScreenOpen(true);
              }}
              organizationSections={
                <OrganizationPanel
                  collections={collections}
                  tags={tags}
                  collectionCounts={collectionCounts}
                  tagCounts={tagCounts}
                  loading={organizationLoading}
                  error={organizationError}
                  selectedCollectionId={selectedCollectionId}
                  selectedTagIds={selectedTagIds}
                  onSelectCollection={handleSelectCollection}
                  onToggleTag={handleToggleTag}
                  onClearFilters={handleClearOrganizationFilters}
                  onRetry={loadOrganization}
                  onCreateCollection={openCreateCollection}
                  onEditCollection={openEditCollection}
                  onDeleteCollection={requestDeleteCollection}
                  onCreateTag={openCreateTag}
                  onEditTag={openEditTag}
                  onDeleteTag={requestDeleteTag}
                />
              }
            />
          )}

          {/* ── Content area ────────────────────────────────────────────────── */}
          <main className="content-area">
            {/* Search + filters toolbar */}
            <div className="content-toolbar">
              <div className="search-bar-row">
                <div className="search-input-wrapper">
                  <Search size={16} className="search-icon" />
                  {searchInput !== searchQuery && (
                    <RefreshCw
                      size={13}
                      className="spin"
                      style={{ color: "var(--text-muted)", marginRight: 6 }}
                      aria-label="Search pending"
                    />
                  )}
                  <input
                    type="text"
                    className="search-input"
                    placeholder="Search file names, extensions, or paths…"
                    value={searchInput}
                    onChange={(e) => setSearchInput(e.target.value)}
                  />
                </div>
              </div>

              {/* Status pills + format selector */}
              <div className="filters-row">
                <div className="status-filter-pills">
                  {(
                    [
                      "all",
                      "indexed",
                      "changed",
                      "missing",
                      "inaccessible",
                    ] as const
                  ).map((s) => (
                    <button
                      key={s}
                      className={`filter-pill ${statusFilter === s ? "active" : ""}`}
                      onClick={() => setStatusFilter(s)}
                    >
                      {s === "all"
                        ? "All Statuses"
                        : s.charAt(0).toUpperCase() + s.slice(1)}
                    </button>
                  ))}
                </div>

                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <select
                    className="filter-select"
                    value={extensionFilter}
                    onChange={(e) => setExtensionFilter(e.target.value)}
                    title="Filter by file extension"
                  >
                    <option value="all">All Formats</option>
                    {distinctExtensions.map((ext) => (
                      <option key={ext} value={ext}>
                        .{ext.toUpperCase()}
                      </option>
                    ))}
                  </select>

                  <div className="result-count-label">
                    {filesLoading
                      ? "Loading…"
                      : `${totalFiles} document${totalFiles !== 1 ? "s" : ""}`}
                  </div>
                </div>
              </div>

              {/* Active collection / tag filter chips */}
              {hasOrganizationFilter && (
                <div className="active-filters-row">
                  <span className="active-filters-label">Filtering by:</span>
                  {activeCollection && (
                    <span className="active-filter-chip">
                      <span
                        className="org-color-dot"
                        style={{ background: activeCollection.color_nullable || "#3b82f6" }}
                      />
                      <span>{activeCollection.name}</span>
                      <button
                        type="button"
                        className="active-filter-chip-remove"
                        onClick={() => setSelectedCollectionId(null)}
                        title="Clear collection filter"
                        aria-label="Clear collection filter"
                      >
                        <X size={11} />
                      </button>
                    </span>
                  )}
                  {activeTags.map((tag) => (
                    <span key={tag.id} className="active-filter-chip">
                      <span
                        className="org-color-dot"
                        style={{ background: tag.color_nullable || "#3b82f6" }}
                      />
                      <span>{tag.name}</span>
                      <button
                        type="button"
                        className="active-filter-chip-remove"
                        onClick={() => handleToggleTag(tag.id)}
                        title={`Clear "${tag.name}" filter`}
                        aria-label={`Clear ${tag.name} filter`}
                      >
                        <X size={11} />
                      </button>
                    </span>
                  ))}
                  <button
                    type="button"
                    className="clear-filters-btn"
                    onClick={handleClearOrganizationFilters}
                  >
                    Clear filters
                  </button>
                </div>
              )}
            </div>

            {/* ── Multi-file organization action bar ─────────────────────── */}
            {selectedFileIds.length > 0 && (
              <div
                className="selection-bar"
                role="region"
                aria-label="Actions for selected files"
              >
                <div className="selection-summary">
                  <SquareCheck size={15} color="#60a5fa" />
                  <span>
                    <strong>{selectedFileIds.length}</strong> file
                    {selectedFileIds.length === 1 ? "" : "s"} selected
                  </span>
                </div>
                <div className="bulk-actions">
                  <button
                    type="button"
                    className="btn btn-secondary"
                    style={{ padding: "4px 9px", fontSize: "0.74rem" }}
                    onClick={() => openBulkAction("collection", "add")}
                    title="Add selected files to a collection (metadata only)"
                  >
                    <FolderPlus size={12} /> Add to collection
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    style={{ padding: "4px 9px", fontSize: "0.74rem" }}
                    onClick={() => openBulkAction("collection", "remove")}
                    title="Remove selected files from a collection (metadata only)"
                  >
                    <FolderMinus size={12} /> Remove from collection
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    style={{ padding: "4px 9px", fontSize: "0.74rem" }}
                    onClick={() => openBulkAction("tag", "add")}
                    title="Add a tag to selected files (metadata only)"
                  >
                    <TagIcon size={12} /> Add tag
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    style={{ padding: "4px 9px", fontSize: "0.74rem" }}
                    onClick={() => openBulkAction("tag", "remove")}
                    title="Remove a tag from selected files (metadata only)"
                  >
                    <Tags size={12} /> Remove tag
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    style={{ padding: "4px 9px", fontSize: "0.74rem" }}
                    onClick={() => setSelectedFileIds([])}
                    title="Clear selection"
                  >
                    <X size={12} /> Clear
                  </button>
                </div>
              </div>
            )}

            {/* Action Alert Banner (e.g. missing file on disk notification) */}
            {actionAlert && (
              <div
                className="stats-banner"
                style={{
                  background:
                    actionAlert.type === "error"
                      ? "rgba(239, 68, 68, 0.15)"
                      : actionAlert.type === "success"
                      ? "rgba(16, 185, 129, 0.15)"
                      : "rgba(245, 158, 11, 0.15)",
                  border: `1px solid ${
                    actionAlert.type === "error"
                      ? "rgba(239, 68, 68, 0.3)"
                      : actionAlert.type === "success"
                      ? "rgba(16, 185, 129, 0.3)"
                      : "rgba(245, 158, 11, 0.3)"
                  }`,
                  color:
                    actionAlert.type === "error"
                      ? "#fca5a5"
                      : actionAlert.type === "success"
                      ? "#6ee7b7"
                      : "#fcd34d",
                }}
              >
                <div className="stats-badges-group">
                  {actionAlert.type === "success" ? (
                    <CheckCircle2 size={15} color="#34d399" />
                  ) : (
                    <AlertTriangle
                      size={15}
                      color={actionAlert.type === "error" ? "#f87171" : "#fbbf24"}
                    />
                  )}
                  <span>{actionAlert.message}</span>
                </div>
                <button
                  className="btn btn-secondary"
                  style={{ padding: "2px 8px", fontSize: "0.72rem" }}
                  onClick={() => setActionAlert(null)}
                >
                  Dismiss
                </button>
              </div>
            )}

            {/* Scan result banner */}
            {scanBanner && (
              <div className="stats-banner">
                <div className="stats-badges-group">
                  <CheckCircle2 size={15} color="#10b981" />
                  <span>{scanBanner.message}</span>
                  {scanBanner.durationMs != null && (
                    <span
                      style={{
                        fontSize: "0.72rem",
                        color: "var(--text-muted)",
                        marginLeft: 6,
                      }}
                    >
                      ({scanBanner.durationMs}ms)
                    </span>
                  )}
                </div>
                <button
                  className="btn btn-secondary"
                  style={{ padding: "2px 8px", fontSize: "0.72rem" }}
                  onClick={() => setScanBanner(null)}
                >
                  Dismiss
                </button>
              </div>
            )}

            {/* File table */}
            <div className="file-table-container">
              {filesLoading ? (
                <div className="loading-state" aria-busy="true">
                  <RefreshCw
                    size={22}
                    className="spin"
                    style={{ color: "var(--text-muted)" }}
                  />
                  <p style={{ color: "var(--text-muted)", marginTop: 10 }}>
                    Loading documents…
                  </p>
                </div>
              ) : files.length > 0 ? (
                <table className="file-table">
                  <thead>
                    <tr>
                      <th
                        style={{ width: 36 }}
                        title="Select files on this page for bulk actions"
                      >
                        <input
                          type="checkbox"
                          className="row-checkbox"
                          checked={allPageSelected}
                          onChange={togglePageSelection}
                          ref={(el) => {
                            if (el) {
                              el.indeterminate = somePageSelected && !allPageSelected;
                            }
                          }}
                          aria-label="Select all files on this page"
                        />
                      </th>
                      <th
                        className="th-sortable"
                        onClick={() => handleSort("name")}
                        title="Sort by file name"
                      >
                        <div className="th-content">
                          <span>Document</span>
                          {renderSortIndicator("name")}
                        </div>
                      </th>
                      <th
                        className="th-sortable"
                        onClick={() => handleSort("extension")}
                        title="Sort by file format"
                        style={{ width: 90 }}
                      >
                        <div className="th-content">
                          <span>Format</span>
                          {renderSortIndicator("extension")}
                        </div>
                      </th>
                      <th
                        className="th-sortable"
                        onClick={() => handleSort("status")}
                        title="Sort by indexing status"
                        style={{ width: 110 }}
                      >
                        <div className="th-content">
                          <span>Status</span>
                          {renderSortIndicator("status")}
                        </div>
                      </th>
                      <th
                        className="th-sortable"
                        onClick={() => handleSort("size")}
                        title="Sort by file size"
                        style={{ width: 110 }}
                      >
                        <div className="th-content">
                          <span>Size</span>
                          {renderSortIndicator("size")}
                        </div>
                      </th>
                      <th
                        className="th-sortable"
                        onClick={() => handleSort("modified")}
                        title="Sort by modified date"
                        style={{ width: 150 }}
                      >
                        <div className="th-content">
                          <span>Modified</span>
                          {renderSortIndicator("modified")}
                        </div>
                      </th>
                      <th style={{ width: 200 }}>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {files.map((file) => (
                      <tr
                        key={file.id}
                        className="file-row clickable-row"
                        onClick={() => handleViewDetail(file.id)}
                        title="Click to view file details"
                      >
                        <td
                          className="file-cell file-cell--select"
                          onClick={(e) => e.stopPropagation()}
                        >
                          <input
                            type="checkbox"
                            className="row-checkbox"
                            checked={selectedFileIds.includes(file.id)}
                            onChange={() => toggleFileSelection(file.id)}
                            aria-label={`Select ${file.file_name}`}
                          />
                        </td>
                        <td className="file-cell">
                          <div className="file-name-cell">
                            <span
                              className={`file-ext-badge ${getExtensionClass(
                                file.extension
                              )}`}
                            >
                              {file.extension || "FILE"}
                            </span>
                            <div style={{ overflow: "hidden" }}>
                              <div style={{ fontWeight: 500 }}>
                                {file.file_name}
                              </div>
                              <div
                                className="path-subtext"
                                title={file.absolute_path}
                              >
                                {file.absolute_path}
                              </div>
                            </div>
                          </div>
                        </td>
                        <td className="file-cell" style={{ textTransform: "uppercase", fontSize: "0.78rem" }}>
                          {file.extension || "—"}
                        </td>
                        <td className="file-cell">
                          {file.status === "indexed" && (
                            <span className="badge badge-active">Indexed</span>
                          )}
                          {file.status === "changed" && (
                            <span
                              className="badge badge-modified"
                              title="Size or timestamp changed on disk"
                            >
                              <AlertTriangle size={12} /> Changed
                            </span>
                          )}
                          {file.status === "moved" && (
                            <span
                              className="badge badge-moved"
                              title="Detected as moved / renamed"
                            >
                              <MoveRight size={12} /> Moved
                            </span>
                          )}
                          {file.status === "missing" && (
                            <span
                              className="badge badge-missing"
                              title="No longer present at recorded path"
                            >
                              Missing
                            </span>
                          )}
                          {file.status === "inaccessible" && (
                            <span
                              className="badge badge-modified"
                              title="File is locked or unreadable"
                            >
                              Inaccessible
                            </span>
                          )}
                          {file.status === "error" && (
                            <span className="badge badge-missing">Error</span>
                          )}
                        </td>
                        <td
                          className="file-cell"
                          style={{ color: "var(--text-secondary)" }}
                        >
                          {formatBytes(file.size_bytes)}
                        </td>
                        <td
                          className="file-cell"
                          style={{ color: "var(--text-secondary)", fontSize: "0.8rem" }}
                        >
                          {formatDate(file.modified_at)}
                        </td>
                        <td className="file-cell">
                          <div
                            className="action-buttons-group"
                            onClick={(e) => e.stopPropagation()}
                          >
                            <button
                              className="btn btn-primary"
                              style={{ padding: "4px 8px", fontSize: "0.74rem" }}
                              onClick={(e) => handleOpenFile(file.id, e)}
                              disabled={file.status === "missing"}
                              title={
                                file.status === "missing"
                                  ? "Cannot open missing file"
                                  : "Open with default application"
                              }
                            >
                              <ExternalLink size={12} /> Open
                            </button>
                            <button
                              className="btn btn-secondary"
                              style={{ padding: "4px 8px", fontSize: "0.74rem" }}
                              onClick={(e) => handleShowInExplorer(file.id, e)}
                              title="Show in Windows File Explorer"
                            >
                              <FolderOpen size={12} /> Explorer
                            </button>
                            <button
                              className="btn btn-secondary"
                              style={{ padding: "4px 8px", fontSize: "0.74rem" }}
                              onClick={(e) => handleViewDetail(file.id, e)}
                              title="View Document Details"
                            >
                              <Info size={12} /> Details
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <div className="empty-state">
                  <FileText className="empty-state-icon" />
                  <h3>No documents found</h3>
                  <p>
                    {hasAnyFilter
                      ? hasOrganizationFilter &&
                        !searchQuery &&
                        statusFilter === "all" &&
                        extensionFilter === "all"
                        ? "No indexed files match the selected collection and tag filters."
                        : "No indexed files match your active filters or search terms."
                      : roots.length === 0
                      ? "Add a folder to begin indexing your documents."
                      : "This folder has not been scanned yet, or contains no files."}
                  </p>
                  <div style={{ marginTop: 12, display: "flex", gap: 8, justifyContent: "center" }}>
                    {hasAnyFilter && (
                      <button
                        className="btn btn-secondary"
                        onClick={() => {
                          setSearchInput("");
                          setStatusFilter("all");
                          setExtensionFilter("all");
                          handleClearOrganizationFilters();
                        }}
                      >
                        Clear Filters & Search
                      </button>
                    )}
                    {roots.length > 0 && !hasAnyFilter && (
                      <button
                        className="btn btn-primary"
                        onClick={handleScanAll}
                        disabled={isScanning}
                      >
                        <RefreshCw size={14} className={isScanning ? "spin" : ""} />
                        {isScanning ? "Scanning…" : "Scan Folders Now"}
                      </button>
                    )}
                  </div>
                </div>
              )}
            </div>

            {/* ── Pagination Footer ────────────────────────────────────────── */}
            {totalFiles > 0 && (
              <div className="pagination-footer">
                <div className="page-size-selector">
                  <span>Show</span>
                  <select
                    className="filter-select"
                    value={pageSize}
                    onChange={(e) => {
                      setPageSize(Number(e.target.value));
                      setPage(1);
                    }}
                  >
                    <option value={10}>10</option>
                    <option value={25}>25</option>
                    <option value={50}>50</option>
                    <option value={100}>100</option>
                  </select>
                  <span>
                    per page · Showing{" "}
                    <strong>{(page - 1) * pageSize + 1}</strong> –{" "}
                    <strong>{Math.min(page * pageSize, totalFiles)}</strong> of{" "}
                    <strong>{totalFiles}</strong> documents
                  </span>
                </div>

                <div className="pagination-controls">
                  <button
                    className="pagination-btn"
                    onClick={() => setPage(1)}
                    disabled={page <= 1}
                    title="First page"
                  >
                    <ChevronsLeft size={14} />
                  </button>
                  <button
                    className="pagination-btn"
                    onClick={() => setPage((p) => Math.max(1, p - 1))}
                    disabled={page <= 1}
                    title="Previous page"
                  >
                    <ChevronLeft size={14} />
                  </button>

                  <span style={{ margin: "0 6px", fontSize: "0.8rem" }}>
                    Page <strong>{page}</strong> of <strong>{totalPages}</strong>
                  </span>

                  <button
                    className="pagination-btn"
                    onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                    disabled={page >= totalPages}
                    title="Next page"
                  >
                    <ChevronRight size={14} />
                  </button>
                  <button
                    className="pagination-btn"
                    onClick={() => setPage(totalPages)}
                    disabled={page >= totalPages}
                    title="Last page"
                  >
                    <ChevronsRight size={14} />
                  </button>
                </div>
              </div>
            )}
          </main>
        </div>
      )}

      {/* ── File Detail Modal ─────────────────────────────────────────────── */}
      <FileDetailModal
        detail={selectedDetail}
        isOpen={detailModalOpen}
        onClose={() => {
          setDetailModalOpen(false);
          setSelectedDetail(null);
        }}
        onFileUpdated={() => {
          loadFiles();
        }}
      />

      {/* ── Export modal ─────────────────────────────────────────────────── */}
      {exportModalOpen && exportData && (
        <div className="modal-overlay" onClick={() => setExportModalOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Download size={18} color="#38bdf8" />
                <h3 style={{ fontSize: "1.05rem" }}>
                  Safe Library Metadata Export
                </h3>
                <span
                  style={{
                    fontSize: "0.72rem",
                    padding: "2px 8px",
                    borderRadius: 9999,
                    background: "rgba(56, 189, 248, 0.15)",
                    color: "#38bdf8",
                    fontWeight: 600,
                  }}
                >
                  v{exportData.format_version}
                </span>
              </div>
              <button
                className="btn btn-danger-ghost"
                onClick={() => setExportModalOpen(false)}
              >
                <X size={18} />
              </button>
            </div>

            <div className="modal-body">
              <p
                style={{
                  fontSize: "0.84rem",
                  color: "var(--text-secondary)",
                }}
              >
                LibOne exports only index metadata — file paths, sizes, and
                timestamps. Document contents are never stored or exported.
              </p>

              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "1fr 1fr 1fr",
                  gap: 12,
                  background: "var(--bg-card)",
                  padding: 12,
                  borderRadius: 8,
                  fontSize: "0.82rem",
                }}
              >
                <div>
                  <div style={{ color: "var(--text-muted)" }}>Total Folders</div>
                  <strong style={{ fontSize: "1.1rem" }}>
                    {exportData.summary.total_roots}
                  </strong>
                </div>
                <div>
                  <div style={{ color: "var(--text-muted)" }}>Total Files</div>
                  <strong style={{ fontSize: "1.1rem" }}>
                    {exportData.summary.total_files}
                  </strong>
                </div>
                <div>
                  <div style={{ color: "var(--text-muted)" }}>Total Size</div>
                  <strong style={{ fontSize: "1.1rem" }}>
                    {formatBytes(exportData.summary.total_size_bytes)}
                  </strong>
                </div>
              </div>

              {exportStatusText && (
                <div
                  style={{
                    color: "#34d399",
                    fontSize: "0.82rem",
                    fontWeight: 500,
                  }}
                >
                  ✓ {exportStatusText}
                </div>
              )}

              <div>
                <div
                  style={{
                    fontSize: "0.78rem",
                    color: "var(--text-muted)",
                    marginBottom: 6,
                  }}
                >
                  JSON Preview:
                </div>
                <div className="json-preview">
                  {JSON.stringify(exportData, null, 2).slice(0, 3000)}
                  {JSON.stringify(exportData, null, 2).length > 3000
                    ? "\n… (truncated preview)"
                    : ""}
                </div>
              </div>
            </div>

            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={handleCopyExportJson}
              >
                {copiedExport ? (
                  <Check size={14} color="#34d399" />
                ) : (
                  <Copy size={14} />
                )}
                {copiedExport ? "Copied JSON" : "Copy to Clipboard"}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleSaveExportToFile}
              >
                <Download size={14} /> Save to File (.json)
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Phase 2 collection & tag management dialogs ──────────────────── */}
      <CollectionModal
        isOpen={collectionModalOpen}
        collection={editingCollection}
        onClose={closeCollectionModal}
        onSaved={handleCollectionSaved}
      />

      <TagModal
        isOpen={tagModalOpen}
        tag={editingTag}
        onClose={closeTagModal}
        onSaved={handleTagSaved}
      />

      <ConfirmDeleteDialog
        isOpen={pendingDelete !== null}
        kind={pendingDelete?.kind ?? "collection"}
        itemName={pendingDelete?.name ?? ""}
        busy={deleteBusy}
        error={deleteError}
        onConfirm={handleConfirmDelete}
        onCancel={closeDeleteDialog}
      />

      {bulkAction && (
        <BulkAssignModal
          isOpen
          kind={bulkAction.kind}
          action={bulkAction.action}
          fileCount={selectedFileIds.length}
          options={bulkOptions}
          loading={organizationLoading}
          onClose={() => setBulkAction(null)}
          onConfirm={handleBulkConfirm}
        />
      )}

      {/* ── Settings / Library Storage modal ───────────────────────────────── */}
      <SettingsModal
        isOpen={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        appInfo={appInfo}
        onOpenExport={handleOpenExportModal}
        onLibraryRestored={handleLibraryRestored}
        onOpenDiagnostics={() => {
          setSettingsOpen(false);
          setDiagnosticsOpen(true);
        }}
      />

      {/* ── Diagnostics modal ──────────────────────────────────────────────── */}
      <DiagnosticsModal
        isOpen={diagnosticsOpen}
        onClose={() => setDiagnosticsOpen(false)}
      />
    </div>
  );
};

export default App;
