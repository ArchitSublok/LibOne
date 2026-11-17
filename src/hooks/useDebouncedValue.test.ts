import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useDebouncedValue } from "./useDebouncedValue";

afterEach(() => {
  vi.useRealTimers();
});

describe("useDebouncedValue", () => {
  it("waits for typing to settle and always returns the latest value", () => {
    vi.useFakeTimers();

    const { result, rerender } = renderHook(
      ({ value }: { value: string }) => useDebouncedValue(value, 300),
      { initialProps: { value: "" } }
    );

    expect(result.current).toBe("");

    rerender({ value: "rep" });
    act(() => vi.advanceTimersByTime(200));
    rerender({ value: "report" });
    act(() => vi.advanceTimersByTime(200));

    // Intermediate input must not trigger a query.
    expect(result.current).toBe("");

    act(() => vi.advanceTimersByTime(100));
    expect(result.current).toBe("report");
  });

  it("returns a settled explicit value after the delay", () => {
    vi.useFakeTimers();

    const { result, rerender } = renderHook(
      ({ value }: { value: number | null }) => useDebouncedValue(value, 300),
      { initialProps: { value: 1 as number | null } }
    );

    rerender({ value: null });
    act(() => vi.advanceTimersByTime(300));
    expect(result.current).toBeNull();
  });
});
