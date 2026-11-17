import React, { useState } from "react";
import {
  AlertCircle,
  ChevronDown,
  ChevronRight,
  Folder,
  Pencil,
  Plus,
  RefreshCw,
  Tag as TagIcon,
  Trash2,
  X,
} from "lucide-react";
import { Collection, Tag } from "../types";

interface Props {
  collections: Collection[];
  tags: Tag[];
  collectionCounts: Record<number, number>;
  tagCounts: Record<number, number>;
  loading: boolean;
  error: string | null;
  selectedCollectionId: number | null;
  selectedTagIds: number[];
  onSelectCollection: (id: number | null) => void;
  onToggleTag: (id: number) => void;
  onClearFilters: () => void;
  onRetry: () => void;
  onCreateCollection: () => void;
  onEditCollection: (collection: Collection) => void;
  onDeleteCollection: (collection: Collection) => void;
  onCreateTag: () => void;
  onEditTag: (tag: Tag) => void;
  onDeleteTag: (tag: Tag) => void;
}

const DEFAULT_COLOR = "#3b82f6";

function countLabel(
  counts: Record<number, number>,
  id: number,
  loading: boolean
): string {
  const value = counts[id];
  if (value === undefined) return loading ? "…" : "0";
  return String(value);
}

/**
 * Sidebar sections for Phase 2 organization: Collections and Tags.
 * Rendered inside the existing Folders sidebar, below the scan roots.
 */
export const OrganizationPanel: React.FC<Props> = ({
  collections,
  tags,
  collectionCounts,
  tagCounts,
  loading,
  error,
  selectedCollectionId,
  selectedTagIds,
  onSelectCollection,
  onToggleTag,
  onClearFilters,
  onRetry,
  onCreateCollection,
  onEditCollection,
  onDeleteCollection,
  onCreateTag,
  onEditTag,
  onDeleteTag,
}) => {
  const [collectionsOpen, setCollectionsOpen] = useState(true);
  const [tagsOpen, setTagsOpen] = useState(true);

  const hasActiveFilter =
    selectedCollectionId !== null || selectedTagIds.length > 0;

  return (
    <div className="organization-panel">
      {error && (
        <div className="inline-error" role="alert">
          <AlertCircle size={13} />
          <span>{error}</span>
          <button
            className="inline-error__dismiss"
            onClick={onRetry}
            title="Retry loading collections and tags"
            aria-label="Retry loading collections and tags"
          >
            <RefreshCw size={12} />
          </button>
        </div>
      )}

      {hasActiveFilter && (
        <div className="org-filter-bar">
          <span className="org-filter-bar-label">Filters active</span>
          <button
            type="button"
            className="org-clear-btn"
            onClick={onClearFilters}
            title="Clear collection and tag filters"
          >
            <X size={11} /> Clear
          </button>
        </div>
      )}

      {/* ── Collections ─────────────────────────────────────────────────── */}
      <section className="sidebar-section" aria-label="Collections">
        <div className="sidebar-section-header">
          <button
            type="button"
            className="sidebar-section-toggle"
            onClick={() => setCollectionsOpen((open) => !open)}
            aria-expanded={collectionsOpen}
          >
            {collectionsOpen ? (
              <ChevronDown size={13} />
            ) : (
              <ChevronRight size={13} />
            )}
            <Folder size={13} />
            <span className="sidebar-section-title">Collections</span>
            <span className="sidebar-section-count">{collections.length}</span>
            {loading && collections.length > 0 && (
              <RefreshCw size={11} className="spin sidebar-section-spinner" />
            )}
          </button>
          <button
            type="button"
            className="sidebar-section-add"
            onClick={onCreateCollection}
            title="Create a new collection"
            aria-label="Create a new collection"
          >
            <Plus size={13} />
          </button>
        </div>

        {collectionsOpen && (
          <div className="sidebar-section-body">
            {error ? (
              <div className="org-empty">
                <p>Could not load collections. Use retry above.</p>
              </div>
            ) : loading && collections.length === 0 ? (
              <>
                <div className="org-skeleton" aria-busy="true">
                  <div className="skeleton-line skeleton-line--wide" />
                  <div className="skeleton-line skeleton-line--narrow" />
                </div>
                <div className="org-skeleton" aria-busy="true">
                  <div className="skeleton-line skeleton-line--wide" />
                  <div className="skeleton-line skeleton-line--narrow" />
                </div>
              </>
            ) : collections.length === 0 ? (
              <div className="org-empty">
                <p>No collections yet. Group related documents without moving them.</p>
                <button
                  type="button"
                  className="btn btn-secondary org-empty-btn"
                  onClick={onCreateCollection}
                >
                  <Plus size={12} /> New collection
                </button>
              </div>
            ) : (
              collections.map((collection) => {
                const selected = selectedCollectionId === collection.id;
                const color = collection.color_nullable || DEFAULT_COLOR;
                return (
                  <div
                    key={collection.id}
                    className={`org-item ${selected ? "active" : ""}`}
                    role="button"
                    tabIndex={0}
                    aria-pressed={selected}
                    title={
                      selected
                        ? `Clear "${collection.name}" filter`
                        : `Filter by "${collection.name}"`
                    }
                    onClick={() =>
                      onSelectCollection(selected ? null : collection.id)
                    }
                    onKeyDown={(e) => {
                      if (e.target !== e.currentTarget) return;
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        onSelectCollection(selected ? null : collection.id);
                      }
                    }}
                  >
                    <span
                      className="org-color-dot"
                      style={{ background: color }}
                      aria-hidden="true"
                    />
                    <span className="org-item-main">
                      <span className="org-item-name">{collection.name}</span>
                      {collection.description_nullable && (
                        <span
                          className="org-item-sub"
                          title={collection.description_nullable}
                        >
                          {collection.description_nullable}
                        </span>
                      )}
                    </span>
                    <span className="org-count-badge">
                      {countLabel(collectionCounts, collection.id, loading)}
                    </span>
                    <span
                      className="org-item-actions"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <button
                        type="button"
                        className="org-action-btn"
                        onClick={() => onEditCollection(collection)}
                        title={`Rename "${collection.name}"`}
                        aria-label={`Rename ${collection.name}`}
                      >
                        <Pencil size={11} />
                      </button>
                      <button
                        type="button"
                        className="org-action-btn org-action-btn--danger"
                        onClick={() => onDeleteCollection(collection)}
                        title={`Delete "${collection.name}" from LibOne`}
                        aria-label={`Delete ${collection.name}`}
                      >
                        <Trash2 size={11} />
                      </button>
                    </span>
                  </div>
                );
              })
            )}
          </div>
        )}
      </section>

      {/* ── Tags ────────────────────────────────────────────────────────── */}
      <section className="sidebar-section" aria-label="Tags">
        <div className="sidebar-section-header">
          <button
            type="button"
            className="sidebar-section-toggle"
            onClick={() => setTagsOpen((open) => !open)}
            aria-expanded={tagsOpen}
          >
            {tagsOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
            <TagIcon size={13} />
            <span className="sidebar-section-title">Tags</span>
            <span className="sidebar-section-count">{tags.length}</span>
            {loading && tags.length > 0 && (
              <RefreshCw size={11} className="spin sidebar-section-spinner" />
            )}
          </button>
          <button
            type="button"
            className="sidebar-section-add"
            onClick={onCreateTag}
            title="Create a new tag"
            aria-label="Create a new tag"
          >
            <Plus size={13} />
          </button>
        </div>

        {tagsOpen && (
          <div className="sidebar-section-body">
            {error ? (
              <div className="org-empty">
                <p>Could not load tags. Use retry above.</p>
              </div>
            ) : loading && tags.length === 0 ? (
              <div className="tag-cloud" aria-busy="true">
                <div className="org-skeleton org-skeleton--pill" />
                <div className="org-skeleton org-skeleton--pill" />
                <div className="org-skeleton org-skeleton--pill" />
              </div>
            ) : tags.length === 0 ? (
              <div className="org-empty">
                <p>No tags yet. Add lightweight labels to documents in place.</p>
                <button
                  type="button"
                  className="btn btn-secondary org-empty-btn"
                  onClick={onCreateTag}
                >
                  <Plus size={12} /> New tag
                </button>
              </div>
            ) : (
              <>
                <div className="tag-cloud">
                  {tags.map((tag) => {
                    const selected = selectedTagIds.includes(tag.id);
                    const color = tag.color_nullable || DEFAULT_COLOR;
                    return (
                      <div
                        key={tag.id}
                        className={`tag-pill ${selected ? "active" : ""}`}
                        style={selected ? { borderColor: color } : undefined}
                      >
                        <button
                          type="button"
                          className="tag-pill-main"
                          onClick={() => onToggleTag(tag.id)}
                          aria-pressed={selected}
                          title={
                            selected
                              ? `Remove "${tag.name}" from the tag filter`
                              : `Filter by "${tag.name}"`
                          }
                        >
                          <span
                            className="org-color-dot"
                            style={{ background: color }}
                            aria-hidden="true"
                          />
                          <span className="tag-pill-name">{tag.name}</span>
                          <span className="tag-pill-count">
                            {countLabel(tagCounts, tag.id, loading)}
                          </span>
                        </button>
                        <span className="tag-pill-actions">
                          <button
                            type="button"
                            className="org-action-btn"
                            onClick={() => onEditTag(tag)}
                            title={`Rename "${tag.name}"`}
                            aria-label={`Rename ${tag.name}`}
                          >
                            <Pencil size={10} />
                          </button>
                          <button
                            type="button"
                            className="org-action-btn org-action-btn--danger"
                            onClick={() => onDeleteTag(tag)}
                            title={`Delete "${tag.name}" from LibOne`}
                            aria-label={`Delete ${tag.name}`}
                          >
                            <Trash2 size={10} />
                          </button>
                        </span>
                      </div>
                    );
                  })}
                </div>
                <div className="org-section-hint">
                  Select multiple tags to match documents that have all of them
                  (AND).
                </div>
              </>
            )}
          </div>
        )}
      </section>
    </div>
  );
};
