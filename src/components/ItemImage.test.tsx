// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { translator } from "../lib/i18n";
import ItemImage from "./ItemImage";

afterEach(cleanup);

const tr = translator("en");

it("tries the public src, then the fallback, then the wfmimg slug", () => {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = { convertFileSrc: (p: string, s: string) => `${s}://localhost/${p}` };
  try {
    const { container } = render(<ItemImage src="https://cdn.example/a.png" fallback="https://cdn.example/b.png" slug="ash_prime_set" tr={tr} />);
    const seen = () => container.querySelector("img")?.getAttribute("src");
    expect(seen()).toBe("https://cdn.example/a.png");
    fireEvent.error(container.querySelector("img")!);
    expect(seen()).toBe("https://cdn.example/b.png");
    fireEvent.error(container.querySelector("img")!);
    expect(seen()).toContain("ash_prime_set");
  } finally {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  }
});
