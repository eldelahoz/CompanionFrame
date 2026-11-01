// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import { resetMock, wfmContractsMy, type WfmContract } from "../../lib/ipc";
import MarketContractRow from "./MarketContractRow";
import ConfirmHost from "../shell/ConfirmHost";
import MarketContracts, { contractUpdate } from "./MarketContracts";

const tr = translator("en");
beforeEach(resetMock);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const first = async (): Promise<WfmContract> => (await wfmContractsMy()).contracts[0];

test("lists own contracts with their stat previews and badge", async () => {
  render(<MarketContracts tr={tr} />);
  const row = await waitFor(() => {
    const r = document.querySelector("[data-market-contract='a1']");
    expect(r).toBeTruthy();
    return r as HTMLElement;
  });
  expect(row.textContent).toContain("Rubico Critacan");
  expect(row.textContent).toContain("Critical Chance");
  expect(row.textContent).toContain("150");
  expect(row.textContent).toContain("Auction");
});

test("removing a contract asks first and then closes the auction", async () => {
  render(
    <>
      <MarketContracts tr={tr} />
      <ConfirmHost tr={tr} />
    </>,
  );
  const remove = async () => fireEvent.click((await screen.findAllByLabelText("Remove"))[0]);
  await remove();
  fireEvent.click(await screen.findByText("Cancel"));
  expect(document.querySelector("[data-market-contract='a1']")).toBeTruthy(); // declined
  await remove();
  fireEvent.click(await screen.findByText("Confirm"));
  await waitFor(() => expect(document.querySelector("[data-market-contract='a1']")).toBeNull());
});

test("hiding keeps the price and flips the button to Unhide", async () => {
  render(<MarketContracts tr={tr} />);
  fireEvent.click((await screen.findAllByLabelText("Hide"))[0]);
  await waitFor(async () => expect((await first()).visible).toBe(false));
  expect((await first()).platinum).toBe(150);
  expect(await screen.findByLabelText("Unhide")).toBeTruthy();
});

test("editing the price saves it and rejects zero", async () => {
  render(<MarketContracts tr={tr} />);
  fireEvent.click((await screen.findAllByLabelText("Edit"))[0]);
  const input = (await screen.findByLabelText("Price")) as HTMLInputElement;
  fireEvent.change(input, { target: { value: "0" } });
  fireEvent.click(screen.getByText("Save"));
  expect(await screen.findByText("Price must be at least 1p")).toBeTruthy();
  fireEvent.change(input, { target: { value: "175" } });
  fireEvent.click(screen.getByText("Save"));
  await waitFor(async () => expect((await first()).platinum).toBe(175));
  expect(document.querySelector("[data-contract-modal]")).toBeNull();
});

test("contractUpdate prices the buyout of an auction and the only price of a direct sale", async () => {
  const c = await first();
  expect(contractUpdate(c, { platinum: 99 })).toMatchObject({ buyoutPrice: 99, startingPrice: 130 });
  expect(contractUpdate({ ...c, isDirectSell: true }, { platinum: 99 })).toMatchObject({ buyoutPrice: 99, startingPrice: null });
  expect(contractUpdate({ ...c, buyoutPlatinum: null }, { platinum: 99 })).toMatchObject({ buyoutPrice: null, startingPrice: 99 });
});

test("a grade renders its letter and attribute label; no grade renders nothing", async () => {
  const c = await first();
  const props = { contract: c, busy: false, onEdit() {}, onToggleVisible() {}, onRemove() {}, tr };
  const { rerender } = render(<table><tbody><MarketContractRow {...props} /></tbody></table>);
  expect(document.querySelector("[data-contract-grade]")).toBeNull();
  rerender(<table><tbody><MarketContractRow {...props} grade={{ overallGrade: "A", attributeGrade: "Great" }} /></tbody></table>);
  expect(document.querySelector("[data-riven-grade='A']")).toBeTruthy();
  expect(document.querySelector("[data-riven-attr-grade='Great']")?.textContent).toBe("Great");
  rerender(<table><tbody><MarketContractRow {...props} grade={null} /></tbody></table>);
  expect(document.querySelector("[data-riven-attr-grade='?']")?.textContent).toBe("Unrated");
});
