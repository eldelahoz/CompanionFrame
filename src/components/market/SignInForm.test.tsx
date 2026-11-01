// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { translator } from "../../lib/i18n";
import SignInForm from "./SignInForm";

const cancel = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("../../lib/ipc", () => ({ wfmSignInWebviewCancel: cancel }));

afterEach(cleanup);

it("offers Cancel only while the webview sign-in is pending, and it closes that window", async () => {
  let finish!: () => void;
  const onSignInWebview = vi.fn(() => new Promise<void>((r) => (finish = r)));
  const { container } = render(<SignInForm onSignIn={vi.fn()} onSignInWebview={onSignInWebview} tr={translator("en")} />);
  expect(container.querySelector("[data-market-signin-cancel]")).toBeNull();
  fireEvent.click(container.querySelector("[data-market-signin-web]")!);
  fireEvent.click(await screen.findByText("Cancel"));
  expect(cancel).toHaveBeenCalledTimes(1);
  finish();
  await vi.waitFor(() => expect(container.querySelector("[data-market-signin-cancel]")).toBeNull());
});
