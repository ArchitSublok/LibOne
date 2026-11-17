import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useResetPageOnFilters } from "./useResetPageOnFilters";

describe("useResetPageOnFilters", () => {
  it("resets to page 1 once per filter-key change", () => {
    const setPage = vi.fn();

    const { rerender } = renderHook(
      ({ filterKey }: { filterKey: string }) =>
        useResetPageOnFilters(filterKey, setPage),
      { initialProps: { filterKey: '["root":null,"status":"all"]' } }
    );

    expect(setPage).toHaveBeenCalledTimes(1);
    expect(setPage).toHaveBeenCalledWith(1);

    rerender({ filterKey: '["root":null,"status":"all"]' });
    expect(setPage).toHaveBeenCalledTimes(1);

    rerender({ filterKey: '["root":null,"status":"missing"]' });
    expect(setPage).toHaveBeenCalledTimes(2);
    expect(setPage).toHaveBeenLastCalledWith(1);
  });
});
