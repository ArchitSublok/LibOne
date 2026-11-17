import { useEffect } from "react";

/**
 * Resets pagination whenever the normalized filter key changes.
 *
 * `filterKey` should be a stable serialization of all active filters and sort
 * options. The initial mount also resets to page 1, which is harmless.
 */
export function useResetPageOnFilters(
  filterKey: string,
  setPage: (page: number) => void
): void {
  useEffect(() => {
    setPage(1);
  }, [filterKey, setPage]);
}
