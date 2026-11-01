// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { translator } from "../../lib/i18n";
import { applyCustomCss, customCss } from "../../lib/customCss/customCssStore";
import CustomCssSection from "./CustomCssSection";

const tr = translator("en");
afterEach(() => {
  cleanup();
  customCss.reset();
});

it("explains a rejected rule, scopes the saved sheet, and applies it only when enabled", () => {
  render(<CustomCssSection tr={tr} />);
  const editor = screen.getByLabelText(tr("customCss.editorLabel"));
  fireEvent.change(editor, { target: { value: "@import url(x.css);\n.card { background: url(http://evil/x.png); color: red; }" } });
  const warnings = document.querySelector("[data-custom-css-warnings]")!.textContent!;
  expect(warnings).toContain(tr("customCss.reason.atImport"));
  expect(warnings).toContain(tr("customCss.reason.remoteUrl"));

  fireEvent.click(screen.getByText(tr("customCss.apply")));
  expect(applyCustomCss().css).toBe("");
  fireEvent.click(screen.getByRole("checkbox"));
  expect(applyCustomCss().css).toContain("#shell .card");
  expect(applyCustomCss().css).not.toContain("evil");
});
